//! `Buffer.from`, `Buffer#write` and `Buffer.byteLength` on base64 / base64url
//! strings under `--node` (#1328). Node decodes a UTF-16 string with the legacy
//! nbytes decoder: each code unit is truncated to its low byte before the
//! alphabet lookup (U+0161 reads as 'a', a high surrogate such as U+D83D reads
//! as '='), a non-alphabet byte is skipped, '=' ends decoding, and the output is
//! capped at the estimate (`from`) or the room left (`write`). EXPECTED is Node
//! 26.8.2's output for SCRIPT (columns: encoding, input, `Buffer.from` hex,
//! `byteLength`, `write` into 8 bytes, `write` into 2 bytes).

use std::process::Command;

const SCRIPT: &str = r#"
var inputs = ["ab\u{1F600}cd", "\u{1F600} emoji", "ab!cd", "ab cd\r\n", "a b=cd",
  "ab=cd", "a=", "=abc", "abcd=efgh", "abc", "a", "ab\u0100cd", "ab\u0161cd",
  "ab\u013dcd", "ab\ud800cd", "ab*cd+/", "ab-_cd", "abcde", "abcdefg",
  "YQ==YQ==", "YW=Jj", "YWJjZA===", "YWJjZA==\nYWJj", "AAAAA", "ab", ""];
var lines = [];
["base64", "base64url"].forEach(function (enc) {
  inputs.forEach(function (s) {
    var b = Buffer.alloc(8);
    var n = b.write(s, enc);
    var c = Buffer.alloc(2);
    var m = c.write(s, enc);
    var shown = JSON.stringify(s).replace(/[^\x20-\x7e]/g, function (ch) {
      return "\\u" + ("0000" + ch.charCodeAt(0).toString(16)).slice(-4);
    });
    lines.push([enc, shown, Buffer.from(s, enc).toString("hex"),
      Buffer.byteLength(s, enc), n + ":" + b.toString("hex"),
      m + ":" + c.toString("hex")].join(" "));
  });
});
console.log(lines.join("\n"));
"#;

const EXPECTED: &str = r#"base64 "ab\ud83d\ude00cd" 69 4 1:6900000000000000 1:6900
base64 "\ud83d\ude00 emoji"  6 0:0000000000000000 0:0000
base64 "ab!cd" 69b71d 3 3:69b71d0000000000 2:69b7
base64 "ab cd\r\n" 69b71d 5 3:69b71d0000000000 2:69b7
base64 "a b=cd" 69 4 1:6900000000000000 1:6900
base64 "ab=cd" 69 3 1:6900000000000000 1:6900
base64 "a="  0 0:0000000000000000 0:0000
base64 "=abc"  3 0:0000000000000000 0:0000
base64 "abcd=efgh" 69b71d 6 3:69b71d0000000000 2:69b7
base64 "abc" 69b7 2 2:69b7000000000000 2:69b7
base64 "a"  0 0:0000000000000000 0:0000
base64 "ab\u0100cd" 69b71d 3 3:69b71d0000000000 2:69b7
base64 "ab\u0161cd" 69b69c 3 3:69b69c0000000000 2:69b6
base64 "ab\u013dcd" 69 3 1:6900000000000000 1:6900
base64 "ab\ud800cd" 69b71d 3 3:69b71d0000000000 2:69b7
base64 "ab*cd+/" 69b71dfb 5 4:69b71dfb00000000 2:69b7
base64 "ab-_cd" 69bfbf71 4 4:69bfbf7100000000 2:69bf
base64 "abcde" 69b71d 3 3:69b71d0000000000 2:69b7
base64 "abcdefg" 69b71d79f8 5 5:69b71d79f8000000 2:69b7
base64 "YQ==YQ==" 61 4 1:6100000000000000 1:6100
base64 "YW=Jj" 61 3 1:6100000000000000 1:6100
base64 "YWJjZA===" 61626364 5 4:6162636400000000 2:6162
base64 "YWJjZA==\nYWJj" 61626364 9 4:6162636400000000 2:6162
base64 "AAAAA" 000000 3 3:0000000000000000 2:0000
base64 "ab" 69 1 1:6900000000000000 1:6900
base64 ""  0 0:0000000000000000 0:0000
base64url "ab\ud83d\ude00cd" 69 4 1:6900000000000000 1:6900
base64url "\ud83d\ude00 emoji"  6 0:0000000000000000 0:0000
base64url "ab!cd" 69b71d 3 3:69b71d0000000000 2:69b7
base64url "ab cd\r\n" 69b71d 5 3:69b71d0000000000 2:69b7
base64url "a b=cd" 69 4 1:6900000000000000 1:6900
base64url "ab=cd" 69 3 1:6900000000000000 1:6900
base64url "a="  0 0:0000000000000000 0:0000
base64url "=abc"  3 0:0000000000000000 0:0000
base64url "abcd=efgh" 69b71d 6 3:69b71d0000000000 2:69b7
base64url "abc" 69b7 2 2:69b7000000000000 2:69b7
base64url "a"  0 0:0000000000000000 0:0000
base64url "ab\u0100cd" 69b71d 3 3:69b71d0000000000 2:69b7
base64url "ab\u0161cd" 69b69c 3 3:69b69c0000000000 2:69b6
base64url "ab\u013dcd" 69 3 1:6900000000000000 1:6900
base64url "ab\ud800cd" 69b71d 3 3:69b71d0000000000 2:69b7
base64url "ab*cd+/" 69b71dfb 5 4:69b71dfb00000000 2:69b7
base64url "ab-_cd" 69bfbf71 4 4:69bfbf7100000000 2:69bf
base64url "abcde" 69b71d 3 3:69b71d0000000000 2:69b7
base64url "abcdefg" 69b71d79f8 5 5:69b71d79f8000000 2:69b7
base64url "YQ==YQ==" 61 4 1:6100000000000000 1:6100
base64url "YW=Jj" 61 3 1:6100000000000000 1:6100
base64url "YWJjZA===" 61626364 5 4:6162636400000000 2:6162
base64url "YWJjZA==\nYWJj" 61626364 9 4:6162636400000000 2:6162
base64url "AAAAA" 000000 3 3:0000000000000000 2:0000
base64url "ab" 69 1 1:6900000000000000 1:6900
base64url ""  0 0:0000000000000000 0:0000"#;

fn run(extra: &[&str], script: &str) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_jsse"))
        .arg("--node")
        .args(extra)
        .args(["-e", script])
        .output()
        .expect("run jsse");
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim_end().to_string()
}

#[test]
fn base64_string_decoding_matches_node() {
    assert_eq!(run(&[], SCRIPT), EXPECTED);
}

#[test]
fn base64_string_decoding_matches_node_in_the_tree_walker() {
    assert_eq!(run(&["--no-bytecode"], SCRIPT), EXPECTED);
}

#[test]
fn a_surrogate_pair_ends_decoding_and_a_leading_one_gives_an_empty_buffer() {
    let script = r#"console.log(
        Buffer.from("ab\u{1F600}cd", "base64").toString("hex"),
        Buffer.from("\u{1F600} emoji", "base64").length,
        Buffer.from("ab\u{1F600}cd", "base64url").toString("hex"),
        new Buffer("ab\u{1F600}cd", "base64").toString("hex"))"#;
    assert_eq!(run(&[], script), "69 0 69 69");
}
