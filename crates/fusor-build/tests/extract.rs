use fusor_build::extract;

#[test]
fn rust_bodies_are_unchanged_and_keep_their_html_line_numbers() {
    let source = "<!doctype html>\n<h1>こんにちは</h1>\n<script type=\"text/rust\">\r\nfn value<T: Clone>(x: &T) -> T {\r\n    let _text = \"&amp; <div> β\";\r\n    x.clone()\r\n}\r\n</script>\n<p>Still HTML &amp; text</p>";
    let page = extract(source).unwrap();
    assert_eq!(page.blocks.len(), 1);
    let body = &source[page.blocks[0].content.clone()];
    assert!(page.rust.contains(body));
    for (original, generated) in source.lines().zip(page.rust.lines()) {
        if original.starts_with("fn ") || original.starts_with("    ") {
            assert_eq!(original, generated);
        }
    }
    assert_eq!(source.lines().count(), page.rust.lines().count());
    assert!(page.html.contains("<h1>こんにちは</h1>"));
    assert!(page.html.contains("<p>Still HTML &amp; text</p>"));
    assert!(!page.html.contains("fn value"));
    assert!(!page.html.contains("text/rust"));
}

#[test]
fn multiple_blocks_share_one_rust_module_in_document_order() {
    let source = r#"<script type="text/rust">const N: usize = 7;</script>
<p>markup between blocks</p>
<script type='text/rust'>fn value() -> usize { N }</script>"#;
    let page = extract(source).unwrap();
    assert_eq!(page.blocks.len(), 2);
    assert!(page.rust.find("const N").unwrap() < page.rust.find("fn value").unwrap());
    assert!(!page.rust.contains("markup between"));
    assert!(page.html.contains("<p>markup between blocks</p>"));
    assert_eq!(page.with_loader().matches("src=\"./boot.js\"").count(), 1);
}

#[test]
fn uses_html_rules_for_case_attribute_quotes_and_entities() {
    let source = "<SCRIPT data-note='>' TYPE = ' Text&#47;Rust '>fn f() {}</SCRIPT >";
    let page = extract(source).unwrap();
    assert_eq!(page.blocks.len(), 1);
    assert_eq!(source[page.blocks[0].content.clone()], *"fn f() {}");
}

#[test]
fn comments_attributes_javascript_and_raw_text_are_not_rust_blocks() {
    let source = r#"<!-- <script type="text/rust">BAD_COMMENT</script> -->
<div data-example='<script type="text/rust">BAD_ATTRIBUTE</script>'></div>
<textarea><script type="text/rust">BAD_TEXTAREA</script></textarea>
<style>/* <script type="text/rust">BAD_STYLE</script> */</style>
<script>const example = '<script type="text/rust">BAD_JS';</script>
<script type="application/json">{"example":"<script type='text/rust'>BAD_JSON"}</script>
<script type="text/rust">fn actual_rust() {}</script>"#;
    let page = extract(source).unwrap();
    assert_eq!(page.blocks.len(), 1);
    assert!(!page.rust.contains("BAD_"));
    assert!(page.rust.contains("fn actual_rust() {}"));
    assert!(page.html.contains("BAD_JS"));
    assert!(page.html.contains("BAD_COMMENT"));
}

#[test]
fn does_not_parse_or_rewrite_invalid_rust() {
    let source = "<script type=text/rust>let broken: i32 = \"wrong\"; fn {</script>";
    let page = extract(source).unwrap();
    assert!(page.rust.contains("let broken: i32 = \"wrong\"; fn {"));
}

#[test]
fn rejects_incomplete_external_self_closing_unclosed_and_ambiguous_blocks() {
    for source in [
        "<script type=text/rust src=app.rs></script>",
        "<script type=text/rust />",
        "<script type=text/rust>fn f() {}",
        "<script type=text/rust type=module>fn f() {}</script>",
    ] {
        assert!(extract(source).is_err(), "accepted {source}");
    }
}

#[test]
fn external_scripts_declare_native_modules_without_copying_authored_rust() {
    let page = extract(
        r#"<script type="text/rust" src="../src/app.rs" rust:module="crate::app"></script>
<App state="{{ App::new(owner) }}"><main>{{ state.count.get() }}</main></App>"#,
    )
    .unwrap();
    let external = page.blocks[0].external.as_ref().unwrap();
    assert_eq!(external.src, "../src/app.rs");
    assert_eq!(external.module, "crate::app");
    assert!(!page.html.contains("rust:module"));
    assert!(!page.html.contains("src/app.rs"));
    assert!(page.rust.contains("__fusor_start"));
    assert!(!page.rust.contains("struct App"));
    for source in [
        r#"<script type="text/rust" rust:module="crate::app"></script>"#,
        r#"<script type="text/rust" src="app.rs" rust:module="app"></script>"#,
        r#"<script type="text/rust" src="app.rs" rust:module="crate::app<T>"></script>"#,
        r#"<script type="text/rust" src="app.rs" rust:module="crate::app">struct App;</script>"#,
        r#"<script type="text/rust" src="https://example.com/app.rs" rust:module="crate::app"></script>"#,
        r#"<script type="text/rust" src="app.rs?raw" rust:module="crate::app"></script>"#,
        r#"<script type="text/rust" src="app.rs" rust:module="crate::app"></script><script type="text/rust">struct App;</script>"#,
        r#"<script type="text/rust" src="app.rs" rust:module="crate::app"></script><script type="text/rust" src="other.rs" rust:module="crate::other"></script>"#,
    ] {
        assert!(extract(source).is_err(), "accepted {source}");
    }
}

#[test]
fn rust_strings_follow_the_html_script_end_delimiter() {
    // HTML ends scripts without inspecting the embedded language's strings.
    let source = r#"<script type="text/rust">const S: &str = "</script>";</script>"#;
    let page = extract(source).unwrap();
    assert_eq!(
        &source[page.blocks[0].content.clone()],
        "const S: &str = \""
    );
    let escaped = r#"<script type="text/rust">const S: &str = "\x3c/script>";</script>"#;
    assert!(extract(escaped).unwrap().rust.contains(r#""\x3c/script>""#));
}

#[test]
fn plain_html_remains_identical_and_has_no_loader() {
    let source = "<!doctype html><title>Plain HTML</title><p>hello</p>";
    let page = extract(source).unwrap();
    assert!(page.blocks.is_empty());
    assert_eq!(page.html, source);
    assert_eq!(page.with_loader(), source);
    assert!(page.rust.trim().is_empty());
}
