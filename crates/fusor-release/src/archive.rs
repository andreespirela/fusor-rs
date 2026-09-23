//! The zip writer is hand-rolled: current `zip` crate releases are
//! pre-releases carrying encryption and several compression backends. This
//! writes the classic format with deflate entries, no zip64. Timestamps are
//! fixed so the same binaries produce the same archive.
use crate::Result;
use std::{
    fs::{self, File},
    io::{Read, Write},
    path::Path,
};

/// 1980-01-01 00:00, the earliest MS-DOS time.
const DOS_EPOCH_TIME: u16 = 0;
const DOS_EPOCH_DATE: u16 = 0x0021;

pub fn tar_gz(directory: &Path, name: &str, archive: &Path) -> Result<()> {
    let file = File::create(archive)?;
    let encoder = flate2::write::GzEncoder::new(file, flate2::Compression::default());
    let mut builder = tar::Builder::new(encoder);
    builder.mode(tar::HeaderMode::Deterministic);
    builder.append_dir_all(name, directory)?;
    builder.into_inner()?.finish()?;
    Ok(())
}

/// Unix modes are recorded, so an executable stays executable when unpacked
/// on a Unix host.
pub fn zip(directory: &Path, name: &str, archive: &Path) -> Result<()> {
    let mut output = Vec::new();
    let mut central = Vec::new();
    let mut entries = 0u16;
    for file in sorted_files(directory)? {
        let entry_name = format!("{name}/{}", file.file_name().to_string_lossy());
        let contents = fs::read(file.path())?;
        let crc = crc32fast::hash(&contents);
        let deflated = deflate(&contents)?;
        let offset = u32::try_from(output.len()).map_err(|_| "archive exceeds 4 GB")?;

        let entry = Entry {
            name: &entry_name,
            crc,
            compressed: deflated.len(),
            uncompressed: contents.len(),
            local_offset: offset,
            executable: executable(&file.path())?,
        };
        write_local_header(&mut output, &entry)?;
        output.extend_from_slice(&deflated);
        write_central_entry(&mut central, &entry)?;
        entries += 1;
    }
    let central_offset = u32::try_from(output.len()).map_err(|_| "archive exceeds 4 GB")?;
    let central_size = u32::try_from(central.len()).map_err(|_| "archive exceeds 4 GB")?;
    output.extend_from_slice(&central);
    write_end_of_central_directory(&mut output, entries, central_size, central_offset)?;
    fs::write(archive, output)?;
    Ok(())
}

/// Inflates every entry and checks its CRC and length.
pub fn verify_zip(archive: &Path, expected_entries: usize) -> Result<()> {
    let bytes = fs::read(archive)?;
    let end = bytes
        .len()
        .checked_sub(22)
        .filter(|start| bytes[*start..*start + 4] == [0x50, 0x4b, 0x05, 0x06])
        .ok_or("no end-of-central-directory record; the archive has a trailing comment or is truncated")?;
    let entries = u16::from_le_bytes([bytes[end + 10], bytes[end + 11]]) as usize;
    if entries != expected_entries {
        return Err(format!("archive holds {entries} entries, expected {expected_entries}").into());
    }
    let mut offset = read_u32(&bytes, end + 16)? as usize;
    for _ in 0..entries {
        if bytes.get(offset..offset + 4) != Some(&[0x50, 0x4b, 0x01, 0x02]) {
            return Err("corrupt central directory".into());
        }
        let crc = read_u32(&bytes, offset + 16)?;
        let compressed = read_u32(&bytes, offset + 20)? as usize;
        let uncompressed = read_u32(&bytes, offset + 24)? as usize;
        let name_length = u16::from_le_bytes([bytes[offset + 28], bytes[offset + 29]]) as usize;
        let local = read_u32(&bytes, offset + 42)? as usize;

        // The local header repeats the name; the data follows its extra field.
        let local_name = u16::from_le_bytes([bytes[local + 26], bytes[local + 27]]) as usize;
        let local_extra = u16::from_le_bytes([bytes[local + 28], bytes[local + 29]]) as usize;
        let start = local + 30 + local_name + local_extra;
        let data = bytes
            .get(start..start + compressed)
            .ok_or("entry data runs past the end of the archive")?;
        let inflated = inflate(data)?;
        if inflated.len() != uncompressed || crc32fast::hash(&inflated) != crc {
            return Err("an archive entry does not match its recorded checksum".into());
        }
        offset += 46 + name_length;
    }
    Ok(())
}

fn sorted_files(directory: &Path) -> Result<Vec<fs::DirEntry>> {
    let mut files: Vec<_> = fs::read_dir(directory)?.collect::<std::io::Result<_>>()?;
    files.retain(|file| file.path().is_file());
    files.sort_by_key(std::fs::DirEntry::file_name);
    Ok(files)
}

#[cfg(unix)]
fn executable(path: &Path) -> Result<bool> {
    use std::os::unix::fs::PermissionsExt;
    Ok(fs::metadata(path)?.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn executable(path: &Path) -> Result<bool> {
    Ok(path.extension().is_some_and(|extension| extension == "exe"))
}

fn deflate(contents: &[u8]) -> Result<Vec<u8>> {
    let mut encoder =
        flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    encoder.write_all(contents)?;
    Ok(encoder.finish()?)
}

fn inflate(contents: &[u8]) -> Result<Vec<u8>> {
    let mut decoder = flate2::read::DeflateDecoder::new(contents);
    let mut out = Vec::new();
    decoder.read_to_end(&mut out)?;
    Ok(out)
}

fn read_u32(bytes: &[u8], at: usize) -> Result<u32> {
    let slice = bytes.get(at..at + 4).ok_or("truncated archive")?;
    Ok(u32::from_le_bytes(slice.try_into().expect("four bytes")))
}

struct Entry<'a> {
    name: &'a str,
    crc: u32,
    compressed: usize,
    uncompressed: usize,
    local_offset: u32,
    executable: bool,
}

fn write_local_header(output: &mut Vec<u8>, entry: &Entry<'_>) -> Result<()> {
    let Entry {
        name,
        crc,
        compressed,
        uncompressed,
        ..
    } = *entry;
    output.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
    output.extend_from_slice(&20u16.to_le_bytes()); // version needed
    output.extend_from_slice(&0u16.to_le_bytes()); // flags
    output.extend_from_slice(&8u16.to_le_bytes()); // deflate
    output.extend_from_slice(&DOS_EPOCH_TIME.to_le_bytes());
    output.extend_from_slice(&DOS_EPOCH_DATE.to_le_bytes());
    output.extend_from_slice(&crc.to_le_bytes());
    output.extend_from_slice(&size(compressed)?.to_le_bytes());
    output.extend_from_slice(&size(uncompressed)?.to_le_bytes());
    output.extend_from_slice(&length(name)?.to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes()); // extra field
    output.extend_from_slice(name.as_bytes());
    Ok(())
}

fn write_central_entry(central: &mut Vec<u8>, entry: &Entry<'_>) -> Result<()> {
    let Entry {
        name,
        crc,
        compressed,
        uncompressed,
        local_offset,
        executable,
    } = *entry;
    // Unix permissions live in the high 16 bits of the external attributes.
    let mode: u32 = if executable { 0o100755 } else { 0o100644 };
    central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
    central.extend_from_slice(&0x031Eu16.to_le_bytes()); // made by Unix, 3.0
    central.extend_from_slice(&20u16.to_le_bytes()); // version needed
    central.extend_from_slice(&0u16.to_le_bytes()); // flags
    central.extend_from_slice(&8u16.to_le_bytes()); // deflate
    central.extend_from_slice(&DOS_EPOCH_TIME.to_le_bytes());
    central.extend_from_slice(&DOS_EPOCH_DATE.to_le_bytes());
    central.extend_from_slice(&crc.to_le_bytes());
    central.extend_from_slice(&size(compressed)?.to_le_bytes());
    central.extend_from_slice(&size(uncompressed)?.to_le_bytes());
    central.extend_from_slice(&length(name)?.to_le_bytes());
    central.extend_from_slice(&0u16.to_le_bytes()); // extra field
    central.extend_from_slice(&0u16.to_le_bytes()); // comment
    central.extend_from_slice(&0u16.to_le_bytes()); // disk number
    central.extend_from_slice(&0u16.to_le_bytes()); // internal attributes
    central.extend_from_slice(&(mode << 16).to_le_bytes());
    central.extend_from_slice(&local_offset.to_le_bytes());
    central.extend_from_slice(name.as_bytes());
    Ok(())
}

fn write_end_of_central_directory(
    output: &mut Vec<u8>,
    entries: u16,
    size: u32,
    offset: u32,
) -> Result<()> {
    output.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes()); // this disk
    output.extend_from_slice(&0u16.to_le_bytes()); // disk with central directory
    output.extend_from_slice(&entries.to_le_bytes());
    output.extend_from_slice(&entries.to_le_bytes());
    output.extend_from_slice(&size.to_le_bytes());
    output.extend_from_slice(&offset.to_le_bytes());
    output.extend_from_slice(&0u16.to_le_bytes()); // comment
    Ok(())
}

fn size(value: usize) -> Result<u32> {
    u32::try_from(value).map_err(|_| "zip entries larger than 4 GB need zip64".into())
}

fn length(name: &str) -> Result<u16> {
    u16::try_from(name.len()).map_err(|_| "archive entry name is too long".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_written_zip_round_trips_and_is_reproducible() {
        let root = std::env::temp_dir().join(format!("fusor-archive-{}", std::process::id()));
        let stage = root.join("stage");
        fs::create_dir_all(&stage).unwrap();
        fs::write(stage.join("fusor"), b"binary contents".repeat(100)).unwrap();
        fs::write(stage.join("LICENSE"), b"license").unwrap();

        let first = root.join("first.zip");
        let second = root.join("second.zip");
        zip(&stage, "fusor-0.1.0-test", &first).unwrap();
        zip(&stage, "fusor-0.1.0-test", &second).unwrap();

        verify_zip(&first, 2).unwrap();
        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());
        fs::remove_dir_all(root).unwrap();
    }
}
