//! Fixed-grid mini-notation: compact rhythm and melody strings.
//!
//! Each top-level token is one grid step (the phrase grid, a 16th by default);
//! the string's length in steps is its length in time.
//!
//! | syntax      | meaning                                              |
//! |-------------|------------------------------------------------------|
//! | `a b c`     | one atom per step                                    |
//! | `.` `~`     | rest                                                 |
//! | `_`         | tie: extend the previous note by a step              |
//! | `[a b]`     | subdivide one step                                   |
//! | `[a,b,c]`   | stack: play together (a chord)                       |
//! | `<a b c>`   | alternate: one per loop cycle                        |
//! | `a*3`       | repeat within the step                               |
//! | `a?`        | 50% chance (seeded)                                  |
//! | `\|`         | ignored — a visual bar line                          |
//!
//! Atoms are opaque text here; the phrase decides what they mean (degrees, note
//! names, roman-numeral chords, drum hits). In [`Mode::Chars`] every character
//! is its own step, so drum grids read naturally: `"x... x.x. ..x. x..."`.

/// How atoms are tokenized.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Atoms are whitespace-separated words: `"1 b3 [5 8]"`, `"i VI III VII"`.
    Words,
    /// Every non-space character is a step: `"x..X x.x."`.
    Chars,
}

#[derive(Clone, Debug, PartialEq)]
enum Node {
    Atom(String),
    Rest,
    Tie,
    Seq(Vec<Node>),
    Stack(Vec<Node>),
    Alt(Vec<Node>),
    Repeat(Box<Node>, u32),
    Maybe(Box<Node>),
}

#[derive(Clone, Debug, PartialEq)]
pub struct ParseError {
    /// 0-based character column.
    pub col: usize,
    pub msg: String,
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "column {}: {}", self.col, self.msg)
    }
}

/// A parsed mini-notation string.
#[derive(Clone, Debug, PartialEq)]
pub struct Pattern {
    steps: Vec<Node>,
}

/// One placed atom, in step units from the pattern start.
#[derive(Clone, Debug, PartialEq)]
pub struct Hit {
    pub start: f32,
    pub dur: f32,
    pub atom: String,
}

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Open,
    Close,
    AltOpen,
    AltClose,
    Comma,
    Rest,
    Tie,
    Star(u32),
    Maybe,
    Atom(String),
}

const SPECIAL: &str = "[]<>,.~_|*?";

fn tokenize(src: &str, mode: Mode) -> Result<Vec<(usize, Tok)>, ParseError> {
    let chars: Vec<char> = src.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let col = i;
        match c {
            c if c.is_whitespace() || c == '|' => {
                i += 1;
            }
            '[' => {
                out.push((col, Tok::Open));
                i += 1;
            }
            ']' => {
                out.push((col, Tok::Close));
                i += 1;
            }
            '<' => {
                out.push((col, Tok::AltOpen));
                i += 1;
            }
            '>' => {
                out.push((col, Tok::AltClose));
                i += 1;
            }
            ',' => {
                out.push((col, Tok::Comma));
                i += 1;
            }
            '.' | '~' => {
                out.push((col, Tok::Rest));
                i += 1;
            }
            '_' => {
                out.push((col, Tok::Tie));
                i += 1;
            }
            '?' => {
                out.push((col, Tok::Maybe));
                i += 1;
            }
            '*' => {
                i += 1;
                let start = i;
                while i < chars.len() && chars[i].is_ascii_digit() {
                    i += 1;
                }
                let n: String = chars[start..i].iter().collect();
                let n: u32 = n.parse().map_err(|_| ParseError { col, msg: "expected a number after `*`".into() })?;
                out.push((col, Tok::Star(n.max(1))));
            }
            _ => match mode {
                Mode::Chars => {
                    out.push((col, Tok::Atom(c.to_string())));
                    i += 1;
                }
                Mode::Words => {
                    let start = i;
                    while i < chars.len() && !chars[i].is_whitespace() && !SPECIAL.contains(chars[i]) {
                        i += 1;
                    }
                    out.push((col, Tok::Atom(chars[start..i].iter().collect())));
                }
            },
        }
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(usize, Tok)>,
    pos: usize,
    end_col: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(_, t)| t)
    }

    fn col(&self) -> usize {
        self.toks.get(self.pos).map(|(c, _)| *c).unwrap_or(self.end_col)
    }

    fn err<T>(&self, msg: &str) -> Result<T, ParseError> {
        Err(ParseError { col: self.col(), msg: msg.into() })
    }

    /// Elements until a closing token or end of input.
    fn seq(&mut self) -> Result<Vec<Node>, ParseError> {
        let mut nodes = Vec::new();
        while let Some(t) = self.peek() {
            if matches!(t, Tok::Close | Tok::AltClose | Tok::Comma) {
                break;
            }
            nodes.push(self.elem()?);
        }
        Ok(nodes)
    }

    fn elem(&mut self) -> Result<Node, ParseError> {
        let mut node = self.primary()?;
        loop {
            match self.peek() {
                Some(Tok::Star(n)) => {
                    let n = *n;
                    self.pos += 1;
                    node = Node::Repeat(Box::new(node), n);
                }
                Some(Tok::Maybe) => {
                    self.pos += 1;
                    node = Node::Maybe(Box::new(node));
                }
                _ => return Ok(node),
            }
        }
    }

    fn primary(&mut self) -> Result<Node, ParseError> {
        let Some(tok) = self.peek().cloned() else {
            return self.err("unexpected end");
        };
        self.pos += 1;
        match tok {
            Tok::Atom(a) => Ok(Node::Atom(a)),
            Tok::Rest => Ok(Node::Rest),
            Tok::Tie => Ok(Node::Tie),
            Tok::Open => {
                let mut layers = vec![self.seq()?];
                while self.peek() == Some(&Tok::Comma) {
                    self.pos += 1;
                    layers.push(self.seq()?);
                }
                if self.peek() != Some(&Tok::Close) {
                    return self.err("expected `]`");
                }
                self.pos += 1;
                Ok(if layers.len() == 1 {
                    Node::Seq(layers.pop().unwrap_or_default())
                } else {
                    Node::Stack(layers.into_iter().map(Node::Seq).collect())
                })
            }
            Tok::AltOpen => {
                let items = self.seq()?;
                if self.peek() != Some(&Tok::AltClose) {
                    return self.err("expected `>`");
                }
                self.pos += 1;
                if items.is_empty() {
                    return self.err("empty `<>`");
                }
                Ok(Node::Alt(items))
            }
            Tok::Close | Tok::AltClose | Tok::Comma | Tok::Star(_) | Tok::Maybe => {
                self.pos -= 1;
                self.err("unexpected symbol")
            }
        }
    }
}

pub fn parse(src: &str, mode: Mode) -> Result<Pattern, ParseError> {
    let toks = tokenize(src, mode)?;
    let mut p = Parser { toks, pos: 0, end_col: src.chars().count() };
    let steps = p.seq()?;
    if p.pos < p.toks.len() {
        return p.err("unexpected symbol");
    }
    Ok(Pattern { steps })
}

impl Pattern {
    /// Length in grid steps.
    pub fn steps(&self) -> usize {
        self.steps.len()
    }

    /// Flatten to placed atoms for loop `cycle`. `chance` answers each `?`.
    pub fn hits(&self, cycle: u32, chance: &mut dyn FnMut() -> bool) -> Vec<Hit> {
        let mut out = Vec::new();
        let mut last = Vec::new();
        for (i, node) in self.steps.iter().enumerate() {
            place(node, i as f32, 1.0, cycle, chance, &mut out, &mut last);
        }
        out
    }
}

fn place(
    node: &Node,
    start: f32,
    dur: f32,
    cycle: u32,
    chance: &mut dyn FnMut() -> bool,
    out: &mut Vec<Hit>,
    last: &mut Vec<usize>,
) {
    match node {
        Node::Atom(a) => {
            out.push(Hit { start, dur, atom: a.clone() });
            last.clear();
            last.push(out.len() - 1);
        }
        Node::Rest => last.clear(),
        Node::Tie => {
            for &i in last.iter() {
                out[i].dur += dur;
            }
        }
        Node::Seq(nodes) => {
            let n = nodes.len().max(1) as f32;
            for (i, child) in nodes.iter().enumerate() {
                place(child, start + i as f32 * dur / n, dur / n, cycle, chance, out, last);
            }
        }
        Node::Stack(layers) => {
            let mut all = Vec::new();
            for layer in layers {
                let mut l = Vec::new();
                place(layer, start, dur, cycle, chance, out, &mut l);
                all.extend(l);
            }
            *last = all;
        }
        Node::Alt(items) => {
            let pick = &items[cycle as usize % items.len()];
            place(pick, start, dur, cycle, chance, out, last);
        }
        Node::Repeat(inner, k) => {
            let k = *k as f32;
            for j in 0..k as usize {
                place(inner, start + j as f32 * dur / k, dur / k, cycle, chance, out, last);
            }
        }
        Node::Maybe(inner) => {
            if chance() {
                place(inner, start, dur, cycle, chance, out, last);
            } else {
                last.clear();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hits(src: &str, mode: Mode, cycle: u32) -> Vec<(f32, f32, String)> {
        parse(src, mode)
            .unwrap()
            .hits(cycle, &mut || true)
            .into_iter()
            .map(|h| (h.start, h.dur, h.atom))
            .collect()
    }

    fn s(x: &str) -> String {
        x.to_string()
    }

    #[test]
    fn steps_rests_and_length() {
        let p = parse("1 . 3 5", Mode::Words).unwrap();
        assert_eq!(p.steps(), 4);
        assert_eq!(hits("1 . 3 5", Mode::Words, 0), vec![(0.0, 1.0, s("1")), (2.0, 1.0, s("3")), (3.0, 1.0, s("5"))]);
    }

    #[test]
    fn subdivision_and_repeat() {
        assert_eq!(hits("[1 2] 3", Mode::Words, 0), vec![(0.0, 0.5, s("1")), (0.5, 0.5, s("2")), (1.0, 1.0, s("3"))]);
        assert_eq!(hits("1*2", Mode::Words, 0), vec![(0.0, 0.5, s("1")), (0.5, 0.5, s("1"))]);
    }

    #[test]
    fn stack_is_simultaneous() {
        assert_eq!(hits("[1,3,5]", Mode::Words, 0), vec![(0.0, 1.0, s("1")), (0.0, 1.0, s("3")), (0.0, 1.0, s("5"))]);
    }

    #[test]
    fn alternation_follows_the_cycle() {
        assert_eq!(hits("<1 2 3>", Mode::Words, 0)[0].2, "1");
        assert_eq!(hits("<1 2 3>", Mode::Words, 1)[0].2, "2");
        assert_eq!(hits("<1 2 3>", Mode::Words, 5)[0].2, "3");
    }

    #[test]
    fn ties_extend_including_stacks() {
        assert_eq!(hits("1 _ _ 2", Mode::Words, 0), vec![(0.0, 3.0, s("1")), (3.0, 1.0, s("2"))]);
        let h = hits("[1,5] _", Mode::Words, 0);
        assert!(h.iter().all(|x| x.1 == 2.0));
        assert_eq!(hits(". _ 1", Mode::Words, 0).len(), 1, "a tie after a rest is silent");
    }

    #[test]
    fn chars_mode_reads_drum_grids() {
        let h = hits("x.X. |xx..", Mode::Chars, 0);
        assert_eq!(h.iter().map(|x| x.0).collect::<Vec<_>>(), vec![0.0, 2.0, 4.0, 5.0]);
        assert_eq!(h[1].2, "X");
        assert_eq!(parse("x... x...", Mode::Chars).unwrap().steps(), 8);
    }

    #[test]
    fn maybe_consults_chance() {
        let p = parse("1? 2?", Mode::Words).unwrap();
        let mut flip = false;
        let h = p.hits(0, &mut || {
            flip = !flip;
            flip
        });
        assert_eq!(h.len(), 1);
    }

    #[test]
    fn roman_atoms_survive_words_mode() {
        let h = hits("I vi <ii vii°> V7/1", Mode::Words, 1);
        assert_eq!(h.iter().map(|x| x.2.as_str()).collect::<Vec<_>>(), vec!["I", "vi", "vii°", "V7/1"]);
    }

    #[test]
    fn errors_report_columns() {
        let e = parse("1 [2 3", Mode::Words).unwrap_err();
        assert_eq!(e.col, 6);
        assert!(parse("1 ] 2", Mode::Words).is_err());
        assert!(parse("<>", Mode::Words).is_err());
    }
}
