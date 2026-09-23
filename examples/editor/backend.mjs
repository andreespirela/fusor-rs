// In-memory demonstration backend. Version check and update run together in one
// event-loop turn. Production persistence must enforce the same check atomically.
// Tests control completion directly, through this object, without HTTP test knobs.
export function projectBackend({ controlled = false } = {}) {
  const initial = () => ({ id: 7, title: "Thursday", body: "A project worth keeping.", seats: 1, version: 10, last_operation: null });
  let project = initial();
  let holdReads = false;
  const writes = [], reads = [], timers = new Set();
  const json = (res, status, value) => {
    if (res.destroyed) return;
    res.writeHead(status, { "content-type": "application/json", "cache-control": "no-store" });
    res.end(JSON.stringify(value));
  };
  function release(write, mode = "accept") {
    if (write.finished) throw new Error("write already completed");
    write.finished = true;
    const { command, res } = write;
    if (command.expected_version !== project.version) {
      write.outcome = "conflict";
      json(res, 409, { message: "Project changed on the server", field: null });
    } else if (command.title.trim().toLowerCase() === "reserved" || mode === "reject") {
      write.outcome = "rejected";
      json(res, 422, { message: "This project title is reserved", field: "title" });
    } else {
      project = { id: 7, title: command.title.trim(), body: command.body, seats: command.seats, version: project.version + 1, last_operation: command.operation };
      write.outcome = "accepted";
      if (mode === "unknown") {
        // Commit happened, then the body was lost. Send headers first so a reused
        // connection closing before response bytes cannot trigger browser replay.
        res.writeHead(200, { "content-type":"application/json", "content-length":"10000", "cache-control":"no-store" });
        res.write("{", () => res.destroy());
      }
      else json(res, 200, project);
    }
  }
  async function handle(req, res) {
    if (req.url.split("?")[0] !== "/editor/api/projects/7") return false;
    if (req.method === "GET") {
      const read = { res, snapshot: structuredClone(project), finished: false };
      reads.push(read);
      if (!holdReads) { read.finished = true; json(res, 200, read.snapshot); }
      return true;
    }
    if (req.method !== "POST") { json(res, 405, { message: "Use GET or POST" }); return true; }
    let body = "";
    try {
      for await (const chunk of req) {
        body += chunk;
        if (body.length > 65_536) { json(res, 413, { message: "Command too large" }); return true; }
      }
      const command = JSON.parse(body);
      if (command.id !== 7 || !Number.isSafeInteger(command.expected_version) || command.expected_version < 0 || typeof command.title !== "string" || typeof command.body !== "string" || typeof command.operation !== "string" || !Number.isInteger(command.seats) || command.seats < 1 || command.seats > 1_000_000 || !command.title.trim()) {
        json(res, 422, { message: "Invalid project command", field: null }); return true;
      }
      const write = { command, res, finished: false, aborted: false, outcome: null };
      writes.push(write);
      res.on("close", () => { if (!res.writableEnded) write.aborted = true; });
      if (!controlled) {
        // Deliberately enough time to demonstrate typing during a save.
        const timer = setTimeout(() => { timers.delete(timer); release(write); }, 900);
        timers.add(timer);
      }
    } catch {
      json(res, 400, { message: "Malformed command", field: null });
    }
    return true;
  }
  return {
    handle, writes, reads, release,
    get project() { return structuredClone(project); },
    set holdReads(value) { holdReads = value; },
    releaseRead(read, value = read.snapshot) { read.finished = true; json(read.res, 200, value); },
    externalEdit(title) { project = { ...project, title, version: project.version + 1, last_operation: null }; },
    reset() {
      for (const timer of timers) clearTimeout(timer); timers.clear();
      for (const item of [...writes, ...reads]) if (!item.finished) item.res.destroy();
      writes.length = 0; reads.length = 0; project = initial(); holdReads = false;
    },
    close() { this.reset(); },
  };
}
