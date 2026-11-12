//! Display filters for captured packets, a practical subset of Wireshark's syntax:
//!
//! - protocol names: `dns`, `tcp`, `tls`, `arp` …
//! - comparisons: `ip.addr == 192.168.1.10`, `tcp.port == 443`, `frame.len > 1000`,
//!   `http.host contains "example"`, `ip.addr == 10.0.0.0/8`
//!   (operators `== != > < >= <= contains`, also `eq ne gt lt ge le`)
//! - logic: `and`/`&&`, `or`/`||`, `not`/`!`, parentheses
//! - a field on its own is true when present and non-zero: `tcp.flags.reset`,
//!   `dns.flags.rcode`
//!
//! A comparison is true when any value of the field matches; `!=` is true when
//! the field is present and none of its values equal the literal.

use std::net::Ipv4Addr;

use crate::packet::{Dissection, FieldValue, KNOWN_FIELDS, PROTOCOLS};

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Or(Box<Expr>, Box<Expr>),
    And(Box<Expr>, Box<Expr>),
    Not(Box<Expr>),
    Protocol(String),
    Field(String),
    Cmp(String, Op, String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Lt,
    Ge,
    Le,
    Contains,
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Word(String),
    Str(String),
    Op(Op),
    And,
    Or,
    Not,
    Open,
    Close,
}

fn tokenize(s: &str) -> Result<Vec<Tok>, String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let two: String = c[i..(i + 2).min(c.len())].iter().collect();
        let (tok, n) = match (c[i], two.as_str()) {
            (ch, _) if ch.is_whitespace() => {
                i += 1;
                continue;
            }
            (_, "==") => (Tok::Op(Op::Eq), 2),
            (_, "!=") => (Tok::Op(Op::Ne), 2),
            (_, ">=") => (Tok::Op(Op::Ge), 2),
            (_, "<=") => (Tok::Op(Op::Le), 2),
            (_, "&&") => (Tok::And, 2),
            (_, "||") => (Tok::Or, 2),
            ('>', _) => (Tok::Op(Op::Gt), 1),
            ('<', _) => (Tok::Op(Op::Lt), 1),
            ('!', _) => (Tok::Not, 1),
            ('(', _) => (Tok::Open, 1),
            (')', _) => (Tok::Close, 1),
            ('"', _) => {
                let end = c[i + 1..]
                    .iter()
                    .position(|&x| x == '"')
                    .ok_or("A quoted value is missing its closing quote")?;
                out.push(Tok::Str(c[i + 1..i + 1 + end].iter().collect()));
                i += end + 2;
                continue;
            }
            (ch, _) if ch.is_alphanumeric() || "._:-/".contains(ch) => {
                let n = c[i..]
                    .iter()
                    .take_while(|x| x.is_alphanumeric() || "._:-/".contains(**x))
                    .count();
                let w: String = c[i..i + n].iter().collect();
                let t = match w.to_ascii_lowercase().as_str() {
                    "and" => Tok::And,
                    "or" => Tok::Or,
                    "not" => Tok::Not,
                    "eq" => Tok::Op(Op::Eq),
                    "ne" => Tok::Op(Op::Ne),
                    "gt" => Tok::Op(Op::Gt),
                    "lt" => Tok::Op(Op::Lt),
                    "ge" => Tok::Op(Op::Ge),
                    "le" => Tok::Op(Op::Le),
                    "contains" => Tok::Op(Op::Contains),
                    _ => Tok::Word(w),
                };
                (t, n)
            }
            (ch, _) => return Err(format!("Unexpected character '{ch}'")),
        };
        out.push(tok);
        i += n;
    }
    Ok(out)
}

struct Parser {
    t: Vec<Tok>,
    i: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.t.get(self.i)
    }
    fn next(&mut self) -> Option<Tok> {
        let t = self.t.get(self.i).cloned();
        self.i += 1;
        t
    }
    fn or(&mut self) -> Result<Expr, String> {
        let mut e = self.and()?;
        while self.peek() == Some(&Tok::Or) {
            self.i += 1;
            e = Expr::Or(Box::new(e), Box::new(self.and()?));
        }
        Ok(e)
    }
    fn and(&mut self) -> Result<Expr, String> {
        let mut e = self.not()?;
        while self.peek() == Some(&Tok::And) {
            self.i += 1;
            e = Expr::And(Box::new(e), Box::new(self.not()?));
        }
        Ok(e)
    }
    fn not(&mut self) -> Result<Expr, String> {
        if self.peek() == Some(&Tok::Not) {
            self.i += 1;
            return Ok(Expr::Not(Box::new(self.not()?)));
        }
        self.primary()
    }
    fn primary(&mut self) -> Result<Expr, String> {
        match self.next() {
            Some(Tok::Open) => {
                let e = self.or()?;
                match self.next() {
                    Some(Tok::Close) => Ok(e),
                    _ => Err("Missing closing parenthesis".into()),
                }
            }
            Some(Tok::Word(w)) => {
                let name = w.to_ascii_lowercase();
                if let Some(Tok::Op(op)) = self.peek().cloned() {
                    self.i += 1;
                    let value = match self.next() {
                        Some(Tok::Word(v) | Tok::Str(v)) => v,
                        _ => {
                            return Err(format!("A value is missing after the operator in '{w} …'"))
                        }
                    };
                    check_field(&name)?;
                    if matches!(op, Op::Gt | Op::Lt | Op::Ge | Op::Le)
                        && parse_num(&value).is_none()
                    {
                        return Err(format!("'{value}' is not a number"));
                    }
                    return Ok(Expr::Cmp(name, op, value));
                }
                if PROTOCOLS.contains(&name.as_str()) {
                    return Ok(Expr::Protocol(name));
                }
                check_field(&name)?;
                Ok(Expr::Field(name))
            }
            Some(Tok::Str(s)) => Err(format!("Expected a field or protocol, found \"{s}\"")),
            Some(_) => Err("Expected a field or protocol".into()),
            None => Err("The filter ends too early".into()),
        }
    }
}

fn check_field(name: &str) -> Result<(), String> {
    if KNOWN_FIELDS.contains(&name) {
        Ok(())
    } else {
        Err(format!("Unknown field '{name}'"))
    }
}

fn parse_num(s: &str) -> Option<u64> {
    match s.strip_prefix("0x").or_else(|| s.strip_prefix("0X")) {
        Some(h) => u64::from_str_radix(h, 16).ok(),
        None => s.parse().ok(),
    }
}

/// Parses a display filter. An empty filter yields `None` (everything matches).
pub fn parse(s: &str) -> Result<Option<Expr>, String> {
    let t = tokenize(s)?;
    if t.is_empty() {
        return Ok(None);
    }
    let mut p = Parser { t, i: 0 };
    let e = p.or()?;
    if p.i < p.t.len() {
        return Err("Unexpected text after the end of the filter (missing 'and' or 'or'?)".into());
    }
    Ok(Some(e))
}

fn cidr(lit: &str) -> Option<(u32, u32)> {
    let (ip, bits) = lit.split_once('/')?;
    let ip: Ipv4Addr = ip.parse().ok()?;
    let bits: u32 = bits.parse().ok().filter(|b| *b <= 32)?;
    let mask = if bits == 0 {
        0
    } else {
        u32::MAX << (32 - bits)
    };
    Some((u32::from(ip) & mask, mask))
}

fn value_eq(v: &FieldValue, lit: &str) -> bool {
    match v {
        FieldValue::Num(n) => parse_num(lit) == Some(*n),
        FieldValue::Text(t) => {
            if let Some((net, mask)) = cidr(lit) {
                return t
                    .parse::<Ipv4Addr>()
                    .is_ok_and(|a| u32::from(a) & mask == net);
            }
            // MACs may be written with '-' or ':'; names and addresses ignore case.
            t.eq_ignore_ascii_case(lit) || t.eq_ignore_ascii_case(&lit.replace('-', ":"))
        }
    }
}

fn cmp(vals: &[FieldValue], op: Op, lit: &str) -> bool {
    match op {
        Op::Eq => vals.iter().any(|v| value_eq(v, lit)),
        Op::Ne => !vals.is_empty() && !vals.iter().any(|v| value_eq(v, lit)),
        Op::Contains => {
            let needle = lit.to_ascii_lowercase();
            vals.iter().any(|v| match v {
                FieldValue::Text(t) => t.to_ascii_lowercase().contains(&needle),
                FieldValue::Num(n) => n.to_string().contains(&needle),
            })
        }
        _ => {
            let Some(n) = parse_num(lit) else {
                return false;
            };
            vals.iter().any(|v| match v {
                FieldValue::Num(x) => match op {
                    Op::Gt => *x > n,
                    Op::Lt => *x < n,
                    Op::Ge => *x >= n,
                    _ => *x <= n,
                },
                FieldValue::Text(_) => false,
            })
        }
    }
}

pub fn matches(e: &Expr, d: &Dissection) -> bool {
    match e {
        Expr::Or(a, b) => matches(a, d) || matches(b, d),
        Expr::And(a, b) => matches(a, d) && matches(b, d),
        Expr::Not(a) => !matches(a, d),
        Expr::Protocol(p) => match p.as_str() {
            "smb" => d.has_protocol("smb") || d.has_protocol("smb2"),
            "data" => d
                .layers
                .last()
                .is_some_and(|l| matches!(l.proto, "tcp" | "udp")),
            _ => d.has_protocol(p),
        },
        Expr::Field(k) => d.values(k).iter().any(|v| match v {
            FieldValue::Num(n) => *n != 0,
            FieldValue::Text(t) => !t.is_empty(),
        }),
        Expr::Cmp(k, op, lit) => cmp(&d.values(k), *op, lit),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::packet::tests::{eth, ipv4, tcp, udp};
    use crate::packet::{dissect, LinkType};

    fn d(b: &[u8]) -> Dissection {
        dissect(b, LinkType::Ethernet, b.len())
    }
    fn m(f: &str, p: &Dissection) -> bool {
        matches(&parse(f).expect("test value").expect("test value"), p)
    }

    #[test]
    fn filters_select_the_right_packets() {
        let https = d(&eth(
            0x0800,
            &ipv4(
                6,
                [192, 168, 1, 169],
                [160, 79, 104, 10],
                &tcp(50000, 443, 0x10, &[]),
            ),
        ));
        let rst = d(&eth(
            0x0800,
            &ipv4(
                6,
                [160, 79, 104, 10],
                [192, 168, 1, 169],
                &tcp(443, 50000, 0x04, &[]),
            ),
        ));
        let mut q = vec![0, 1, 1, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        q.extend_from_slice(b"\x06claude\x02ai\x00\x00\x01\x00\x01");
        let dnsq = d(&eth(
            0x0800,
            &ipv4(17, [192, 168, 1, 169], [1, 1, 1, 1], &udp(5000, 53, &q)),
        ));

        assert!(m("tcp", &https) && !m("udp", &https));
        assert!(m("ip.addr == 160.79.104.10", &https));
        assert!(m("ip.dst == 160.79.104.10 && tcp.port == 443", &https));
        assert!(m("ip.addr == 192.168.0.0/16", &https));
        assert!(!m("ip.addr == 10.0.0.0/8", &https));
        assert!(m("tcp.flags.reset", &rst) && !m("tcp.flags.reset", &https));
        assert!(m("tcp.flags.reset == 1 or dns.flags.rcode > 0", &rst));
        assert!(m("dns and dns.qry.name contains \"CLAUDE\"", &dnsq));
        assert!(m("not tcp", &dnsq));
        assert!(m("!(tcp.port == 80) and port eq 443", &https));
        assert!(m("ip.src != 1.1.1.1", &https));
        assert!(!m("tcp.port != 443", &https), "no value may equal for !=");
        assert!(!m("dns.qry.name != x", &https), "!= needs the field");
        assert!(m("eth.src == 02-1a-2b-3c-4d-5e", &https));
        assert!(m("frame.len >= 54 and frame.len < 0x100", &https));
    }

    #[test]
    fn helpful_errors() {
        assert_eq!(parse("").expect("test value"), None);
        assert_eq!(parse("   ").expect("test value"), None);
        assert_eq!(
            parse("ip.adr == 1.2.3.4").expect_err("invalid filter"),
            "Unknown field 'ip.adr'"
        );
        assert_eq!(
            parse("tcp.port >").expect_err("invalid filter"),
            "A value is missing after the operator in 'tcp.port …'"
        );
        assert_eq!(
            parse("frame.len > big").expect_err("invalid filter"),
            "'big' is not a number"
        );
        assert!(parse("(tcp")
            .expect_err("invalid filter")
            .contains("parenthesis"));
        assert!(parse("tcp udp")
            .expect_err("invalid filter")
            .contains("missing 'and'"));
        assert!(parse("http.host contains \"x")
            .expect_err("invalid filter")
            .contains("quote"));
        assert!(parse("tcp $").expect_err("invalid filter").contains("'$'"));
    }
}
