//! The embedded `url`, `querystring` and `punycode` modules and the WHATWG
//! `URL` / `URLSearchParams` globals under `--node` (#1230). The Node-oracle
//! diff is `scripts/run-node-url-oracle.sh`; this pins the digest of that
//! corpus and checks the opt-in gate, the lazy globals and the error shapes.

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("scripts/url-fixtures")
}

fn run(args: &[&str], gc_stress: Option<&str>) -> Output {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_jsse"));
    if let Some(period) = gc_stress {
        cmd.env("JSSE_GC_STRESS", period);
    }
    cmd.args(args).output().expect("run jsse")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).trim_end().to_string()
}

struct Scratch(PathBuf);

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn scratch(tag: &str) -> Scratch {
    let dir = std::env::temp_dir().join(format!("jsse-node-url-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    Scratch(dir)
}

fn corpus(dir: &Path) -> PathBuf {
    let mut names: Vec<String> = std::fs::read_dir(fixtures())
        .expect("fixtures dir")
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.starts_with("corpus-") && n.ends_with(".js"))
        .collect();
    names.sort();
    assert!(names.len() >= 8, "missing corpus parts: {names:?}");
    let mut script = String::new();
    for name in names {
        script.push_str(&std::fs::read_to_string(fixtures().join(name)).unwrap());
    }
    let path = dir.join("corpus.js");
    std::fs::write(&path, script).unwrap();
    path
}

fn eval(source: &str, flags: &[&str], gc_stress: Option<&str>) -> String {
    let mut args = vec!["--node"];
    args.extend_from_slice(flags);
    args.extend_from_slice(&["-e", source]);
    let out = run(&args, gc_stress);
    assert!(out.status.success(), "{}", stderr(&out));
    stdout(&out)
}

#[test]
fn corpus_digest_matches_node() {
    let want = std::fs::read_to_string(fixtures().join("corpus.digest")).unwrap();
    let dir = scratch("corpus");
    let script = corpus(&dir.0);
    for flags in [&[][..], &["--no-bytecode"][..]] {
        let mut args = vec!["--node"];
        args.extend_from_slice(flags);
        args.push(script.to_str().unwrap());
        let out = run(&args, None);
        assert!(out.status.success(), "{flags:?}: {}", stderr(&out));
        assert_eq!(stderr(&out), "", "{flags:?}");
        assert_eq!(
            stdout(&out).lines().last().unwrap(),
            want.trim_end(),
            "{flags:?}"
        );
    }
}

#[test]
fn url_globals_are_absent_without_node() {
    let probe = r#"print(typeof URL, typeof URLSearchParams, typeof __host_idna_to_ascii, typeof __host_idna_to_unicode)"#;
    let out = run(&["-e", probe], None);
    assert_eq!(stdout(&out), "undefined undefined undefined undefined");
    let dir = scratch("gate");
    let empty = dir.0.join("empty.js");
    std::fs::write(&empty, "void 0;").unwrap();
    let out = run(&["--prelude", empty.to_str().unwrap(), "-e", probe], None);
    assert_eq!(stdout(&out), "undefined undefined undefined undefined");
}

#[test]
fn globals_are_non_enumerable_and_settle_into_data_properties() {
    let source = r#"
      const before = ["URL", "URLSearchParams"].map((n) => {
        const d = Object.getOwnPropertyDescriptor(globalThis, n);
        return [typeof d.get, d.enumerable, d.configurable].join();
      });
      print(before.join(" "));
      print(typeof __host_idna_to_ascii, typeof __host_idna_to_unicode, Object.keys(globalThis).filter((k) => /^URL/.test(k)).length);
      const u = new URL("http://a/b?c");
      const d = Object.getOwnPropertyDescriptor(globalThis, "URL");
      print(typeof d.value, d.writable, d.enumerable, d.configurable, URL === __node_builtin("url").URL, u instanceof URL);
      URLSearchParams = 5;
      const e = Object.getOwnPropertyDescriptor(globalThis, "URLSearchParams");
      print(e.value, e.writable, e.enumerable, e.configurable);
    "#;
    assert_eq!(
        eval(source, &[], None),
        "function,false,true function,false,true\n\
         undefined undefined 0\n\
         function true false true true true\n\
         5 true false true"
    );
}

#[test]
fn whatwg_parsing_setters_and_search_params() {
    let source = r#"
      const u = new URL("https://user:pa ss@EXAMPLE.com:8080/a/./b/../c?x=1&y=2#frag");
      print(u.href, u.origin, u.host);
      u.searchParams.append("z", "a b");
      u.port = "443"; u.hash = ""; u.pathname = "/\u00fc";
      print(u.href, [...u.searchParams].join("|"));
      print(new URL("../x?y#z", "http://a/b/c/d").href, new URL("//h/p", "ftp://x/").href);
      print(new URL("http://\u4f60\u597d.com/").hostname, new URL("http://fa\u00df.de").host, new URL("http://0x7f.1").host);
      print(new URL("http://[1:0:0:0:0:0:0:1]/").host, new URL("file:///C:/a/../b").pathname, new URL("data:a b ?c").href);
      print(URL.canParse("nope"), URL.canParse("x", "http://a"), URL.parse("nope"), URL.parse("/x", "http://a").href);
      print(JSON.stringify(new URL("http://a/")), `${new URL("http://a/b")}`, Object.prototype.toString.call(u));
    "#;
    assert_eq!(
        eval(source, &[], None),
        "https://user:pa%20ss@example.com:8080/a/c?x=1&y=2#frag https://example.com:8080 example.com:8080\n\
         https://user:pa%20ss@example.com/%C3%BC?x=1&y=2&z=a+b x,1|y,2|z,a b\n\
         http://a/b/x?y#z ftp://h/p\n\
         xn--6qq79v.com xn--fa-hia.de 127.0.0.1\n\
         [1::1] /C:/b data:a b%20?c\n\
         false true null http://a/x\n\
         \"http://a/\" http://a/b [object URL]"
    );
}

#[test]
fn invalid_urls_throw_node_shaped_errors() {
    let source = r#"
      for (const args of [["nope"], ["x", "bad base"], ["http://a b/"]]) {
        try { new URL(...args); } catch (e) {
          print(e.name, e.code, e.message, e.input, e.base, Object.keys(e).join());
        }
      }
      try { new URL(); } catch (e) { print(e.code, e.message); }
      try { new URLSearchParams([["a"]]); } catch (e) { print(e.code, e.message); }
      try { new URLSearchParams().append("a"); } catch (e) { print(e.code, e.message); }
    "#;
    assert_eq!(
        eval(source, &[], None),
        "TypeError ERR_INVALID_URL Invalid URL nope undefined code,input\n\
         TypeError ERR_INVALID_URL Invalid URL x bad base code,input,base\n\
         TypeError ERR_INVALID_URL Invalid URL http://a b/ undefined code,input\n\
         ERR_MISSING_ARGS The \"url\" argument must be specified\n\
         ERR_INVALID_TUPLE Each query pair must be an iterable [name, value] tuple\n\
         ERR_MISSING_ARGS The \"name\" and \"value\" arguments must be specified"
    );
}

#[test]
fn file_urls_round_trip_in_posix_and_win32_forms() {
    let source = r#"
      const url = __node_builtin("url");
      const rows = [
        ["/a b/\u00fc#?%", false], ["C:\\dir\\f g.txt", true], ["\\\\srv\\share\\x", true], ["/a\\b", false],
      ];
      for (const [p, windows] of rows) {
        const href = url.pathToFileURL(p, { windows }).href;
        print(JSON.stringify(p), href, JSON.stringify(url.fileURLToPath(href, { windows })));
      }
      for (const [href, windows] of [["file:///a%2Fb", false], ["file:///C:/a%5Cb", true], ["file://h/x", false], ["http://x/", false]]) {
        try { url.fileURLToPath(href, { windows }); } catch (e) { print(e.code, e.message); }
      }
    "#;
    assert_eq!(
        eval(source, &[], None),
        "\"/a b/\u{fc}#?%\" file:///a%20b/%C3%BC%23%3F%25 \"/a b/\u{fc}#?%\"\n\
         \"C:\\\\dir\\\\f g.txt\" file:///C:/dir/f%20g.txt \"C:\\\\dir\\\\f g.txt\"\n\
         \"\\\\\\\\srv\\\\share\\\\x\" file://srv/share/x \"\\\\\\\\srv\\\\share\\\\x\"\n\
         \"/a\\\\b\" file:///a%5Cb \"/a\\\\b\"\n\
         ERR_INVALID_FILE_URL_PATH File URL path must not include encoded / characters\n\
         ERR_INVALID_FILE_URL_PATH File URL path must not include encoded \\ or / characters\n\
         ERR_INVALID_FILE_URL_HOST File URL host must be \"localhost\" or empty on linux\n\
         ERR_INVALID_URL_SCHEME The URL must be of scheme file"
    );
}

#[test]
fn legacy_url_querystring_and_punycode() {
    let source = r#"
      const url = __node_builtin("node:url");
      const qs = __node_builtin("querystring");
      const p = __node_builtin("punycode");
      const parsed = url.parse("http://u:p@Host.com:81/a/b?x=1&y=2#h", true);
      print(parsed.host, parsed.auth, parsed.pathname, JSON.stringify(parsed.query), parsed.hash, url.format(parsed));
      print(url.resolve("http://a/b/c/d", "../e"), url.resolve("/one/two", "three"), url.format({ protocol: "https", hostname: "x.y", pathname: "/p", query: { a: [1, 2] } }));
      print(url.domainToASCII("fa\u00df.de"), url.domainToUnicode("xn--fiq228c.com"), JSON.stringify(url.urlToHttpOptions(new URL("http://u:p@h:9/p?q"))));
      print(qs.stringify({ a: [1, 2], b: "c d" }), JSON.stringify(qs.parse("a=1&a=2&b=%20")), qs.escape("a b&"), qs.unescape("%41%zz"));
      print(p.encode("m\u00fcnchen"), p.toASCII("m\u00fcnchen.de"), p.toUnicode("xn--mnchen-3ya.de"));
      print(url.format(new URL("http://u:p@xn--fiq228c.com/p?q#h"), { unicode: true, auth: false, fragment: false }));
    "#;
    assert_eq!(
        eval(source, &[], None),
        "host.com:81 u:p /a/b {\"x\":\"1\",\"y\":\"2\"} #h http://u:p@host.com:81/a/b?x=1&y=2#h\n\
         http://a/b/e /one/three https://x.y/p?a=1&a=2\n\
         xn--fa-hia.de \u{4e2d}\u{6587}.com {\"protocol\":\"http:\",\"hostname\":\"h\",\"hash\":\"\",\"search\":\"?q\",\"pathname\":\"/p\",\"path\":\"/p?q\",\"href\":\"http://u:p@h:9/p?q\",\"port\":9,\"auth\":\"u:p\"}\n\
         a=1&a=2&b=c%20d {\"a\":[\"1\",\"2\"],\"b\":\" \"} a%20b%26 A%zz\n\
         mnchen-3ya xn--mnchen-3ya.de m\u{fc}nchen.de\n\
         http://\u{4e2d}\u{6587}.com/p?q"
    );
}

#[test]
fn url_works_under_the_tree_walker_and_gc_stress() {
    let source = r#"
      const u = new URL("http://user:pw@exa\u00dfmple.com:81/a/b?x=1#f");
      u.searchParams.append("y", "\u{1f600}"); u.hostname = "\u4f60\u597d.com"; u.pathname = "/\u00fc";
      const q = new URLSearchParams("b=2&a=1&b=1"); q.sort();
      print(u.href, q.toString(), __node_builtin("url").parse("//h/x?y").query);
    "#;
    let want = "http://user:pw@xn--6qq79v.com:81/%C3%BC?x=1&y=%F0%9F%98%80#f a=1&b=2&b=1 y";
    assert_eq!(eval(source, &["--no-bytecode"], None), want);
    assert_eq!(eval(source, &[], Some("7")), want);
}
