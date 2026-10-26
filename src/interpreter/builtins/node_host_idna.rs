//! UTS #46 for Node host mode (issue #1230): the two `__host_idna_*` floor
//! primitives behind the WHATWG URL host parser and `url.domainTo*`. Everything
//! else about hosts (percent-decoding, forbidden code points, IPv4/IPv6) is
//! JavaScript in `node-lib/internal/url/`. Nothing here exists without `--node`.

use super::super::*;
use idna::uts46::{AsciiDenyList, DnsLength, Hyphens, Uts46};

/// UTS #46 ToASCII with the WHATWG URL parameters (CheckHyphens=false,
/// UseSTD3ASCIIRules=false, VerifyDnsLength=false, non-transitional), applied
/// label by label as ada does: an ASCII label other than `xn--...` is only
/// lowercased, every other label goes through full validation on its own
/// (so the Bidi rule sees one label, not the whole domain).
fn domain_to_ascii(domain: &str) -> Option<String> {
    let uts46 = Uts46::new();
    let mut out = String::with_capacity(domain.len());
    for (i, label) in domain
        .split(['.', '\u{3002}', '\u{ff0e}', '\u{ff61}'])
        .enumerate()
    {
        if i > 0 {
            out.push('.');
        }
        let is_punycode = label.len() >= 4
            && label.is_char_boundary(4)
            && label[..4].eq_ignore_ascii_case("xn--");
        if label.is_ascii() && !is_punycode {
            out.push_str(&label.to_ascii_lowercase());
            continue;
        }
        let ascii = uts46
            .to_ascii(
                label.as_bytes(),
                AsciiDenyList::EMPTY,
                Hyphens::Allow,
                DnsLength::Ignore,
            )
            .ok()?;
        out.push_str(&ascii);
    }
    Some(out)
}

/// Label-wise ToUnicode: a label that fails validation is kept as given, as
/// ada does, instead of being marked with U+FFFD.
fn domain_to_unicode(domain: &str) -> String {
    let uts46 = Uts46::new();
    domain
        .split('.')
        .map(|label| {
            if !label.is_ascii() || label.len() < 4 || !label[..4].eq_ignore_ascii_case("xn--") {
                return label.to_string();
            }
            match uts46.to_unicode(label.as_bytes(), AsciiDenyList::EMPTY, Hyphens::Allow) {
                (out, Ok(())) => out.into_owned(),
                (_, Err(_)) => label.to_string(),
            }
        })
        .collect::<Vec<_>>()
        .join(".")
}

impl Interpreter {
    pub(super) fn setup_node_host_idna(&mut self) {
        let to_ascii = self.create_function(JsFunction::native(
            "__host_idna_to_ascii".to_string(),
            1,
            |interp, _this, args| {
                let s = match interp
                    .to_string_lossy(args.first().unwrap_or(JsValue::undefined_ref()))
                {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                Completion::Normal(domain_to_ascii(&s).map_or(JsValue::UNDEFINED, |out| {
                    JsValue::string(JsString::from_str(&out))
                }))
            },
        ));
        self.install_host_global("__host_idna_to_ascii", to_ascii);

        let to_unicode = self.create_function(JsFunction::native(
            "__host_idna_to_unicode".to_string(),
            1,
            |interp, _this, args| {
                let s = match interp
                    .to_string_lossy(args.first().unwrap_or(JsValue::undefined_ref()))
                {
                    Ok(s) => s,
                    Err(e) => return Completion::Throw(e),
                };
                Completion::Normal(JsValue::string(JsString::from_str(&domain_to_unicode(&s))))
            },
        ));
        self.install_host_global("__host_idna_to_unicode", to_unicode);
    }
}
