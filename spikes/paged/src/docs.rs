//! The documents the spike opens: the long generated one, and short ones that hold what each
//! check looks at. `PAGED_DOC` picks one (`from_env`).

use rune::color::hex;
use rune::{Baseline, Block, Mark, MarkColor, MarkSize, ParagraphAlign};

use crate::{BODY, CHAPTER, Doc, HEADING, OTHER_FAMILY, Para, TIGHT, TITLE};

/// A locked atom's label and height, as Fude labels content it keeps but cannot edit yet.
pub const ATOM: &str = "Tabelle – in dieser Version nicht bearbeitbar";
pub const ATOM_HEIGHT: f32 = 120.0;

/// Sections a chapter holds.
const SECTIONS: usize = 14;

const WORDS: &[&str] = &[
    "Hafen", "Morgen", "Brücke", "leise", "Fenster", "Garten", "über", "der", "die", "das", "und", "mit", "einem", "Zug", "Regen", "Straße",
    "langsam", "Lampe", "Tisch", "Brief", "schreibt", "liest", "Papier", "Tinte", "Feder", "zwischen", "den", "alten", "Häusern", "wartet",
    "Nachbarin", "Markt", "Glocke", "Turm", "Schiff", "Kapitän", "Karte", "Küste", "Wind", "nach", "Norden", "Stunde", "Uhr", "schlägt",
    "sieben", "Kaffee", "Bäcker", "Brot", "warm", "Kinder", "laufen", "Schule", "Lehrerin", "Kreide", "Tafel", "Rechnung", "Zahl", "Ordner",
    "Amt", "Stempel", "Antrag", "Frist", "Montag", "Dienstag", "Winter", "Schnee", "Dach", "Rauch", "Ofen", "Holz", "Wald", "Weg", "Fluss",
    "Ufer", "Stein", "Moos", "Vogel", "ruft", "weit", "hinter", "dem", "Hügel", "Dorf", "Kirche", "Platz", "Brunnen", "Wasser", "klar",
    "kalt", "Abend", "Licht", "geht", "an", "in", "einer", "Küche", "Suppe", "Löffel", "Teller", "Gespräch", "Frage", "Antwort", "vielleicht",
    "später", "noch", "nicht", "immer", "wieder", "kommt", "bleibt", "sagt", "hört", "sieht", "kleine", "große", "neue", "gelbe", "stille",
];

struct Rng(u64);

impl Rng {
    fn next(&mut self, n: usize) -> usize {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 % n as u64) as usize
    }

    /// A sentence of `n` words, and where each word starts.
    fn sentence(&mut self, n: usize, out: &mut String, starts: &mut Vec<usize>) {
        for i in 0..n {
            if !out.is_empty() { out.push(' '); }
            starts.push(out.len());
            let w = WORDS[self.next(WORDS.len())];
            if i == 0 {
                let mut c = w.chars();
                if let Some(f) = c.next() { out.extend(f.to_uppercase()); out.push_str(c.as_str()); }
            } else {
                out.push_str(w);
            }
        }
        out.push('.');
    }
}

fn red() -> MarkColor {
    MarkColor::new(hex("#b3261e").unwrap_or(rune::Color::WHITE))
}

fn yellow() -> MarkColor {
    MarkColor::new(hex("#ffe14d").unwrap_or(rune::Color::WHITE))
}

/// The same document every time: a title, then chapters of headed sections, about `words`
/// words in all. Every chapter after the first breaks the page before it: the break is the
/// paragraph's own, not its style's.
pub fn generate(words: usize) -> Doc {
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    let mut paras = vec![Para::new("Der Hafen am Morgen", TITLE)];
    let (mut count, mut chapter, mut body_no) = (4usize, 0usize, 0usize);
    while count < words {
        chapter += 1;
        let (mut t, mut s) = (format!("Kapitel {chapter}:"), Vec::new());
        rng.sentence(3, &mut t, &mut s);
        t.pop();
        count += 5;
        paras.push(if chapter == 1 { Para::new(t, HEADING) } else { Para::new(t, CHAPTER).attrs(|a| a.page_break_before(true)) });
        for section in 0..SECTIONS {
            if count >= words { break; }
            if section > 0 {
                let (mut t, mut s) = (String::new(), Vec::new());
                let n = 2 + rng.next(3);
                rng.sentence(n, &mut t, &mut s);
                t.pop();
                count += s.len();
                paras.push(Para::new(t, HEADING));
            }
            for _ in 0..4 + rng.next(3) {
                let (mut t, mut starts) = (String::new(), Vec::new());
                for _ in 0..3 + rng.next(5) {
                    let n = 6 + rng.next(14);
                    rng.sentence(n, &mut t, &mut starts);
                }
                count += starts.len();
                body_no += 1;
                // Runs of a few words: bold, a colour, a larger size.
                let run = |from: usize, len: usize| (starts[from.min(starts.len() - 1)], starts.get(from + len).map_or(t.len(), |&e| e - 1));
                let mut marks = Vec::new();
                if body_no % 3 == 0 { let (a, b) = run(2, 3); marks.push((a, b, Mark::Bold)); }
                if body_no % 4 == 1 { let (a, b) = run(9, 4); marks.push((a, b, Mark::Color(red()))); }
                if body_no % 5 == 2 { let (a, b) = run(14, 2); marks.push((a, b, Mark::Size(MarkSize::pt(15.0)))); }
                if body_no % 7 == 3 { let (a, b) = run(5, 2); marks.push((a, b, Mark::Italic)); }
                marks.retain(|m| m.0 < m.1);
                let mut p = Para::new(t, BODY);
                p.marks = marks;
                paras.push(p);
            }
        }
    }
    Doc::of(paras)
}

/// A paragraph of plain words about `lines` lines long at the page's width.
fn filler(seed: u64, words: usize) -> String {
    let mut rng = Rng(seed);
    let (mut t, mut s) = (String::new(), Vec::new());
    rng.sentence(words, &mut t, &mut s);
    t
}

/// Run marks and paragraph attributes, one of each on a short page.
pub fn features() -> Doc {
    let long = filler(7, 60);
    Doc::of(vec![
        Para::new("Zeichen und Absätze", TITLE),
        Para::new("Schrift: MMMMiiii in der Grundschrift, MMMMiiii in einer anderen, MMMMiiii in einer unbekannten.", BODY)
            .mark_at("MMMMiiii in einer anderen", 8, Mark::family(OTHER_FAMILY))
            .mark_at("MMMMiiii in einer unbekannten", 8, Mark::family("Keine Solche Schrift")),
        Para::new("Marker: ein gelb markiertes Wort, eines groß und markiert.", BODY)
            .mark("markiertes", Mark::Highlight(yellow()))
            .mark("groß und markiert", Mark::Highlight(yellow()))
            .mark("groß", Mark::Size(MarkSize::pt(18.0))),
        Para::new("Formel: H2O und E = mc2 stehen tief und hoch.", BODY)
            .mark_at("2O", 1, Mark::Baseline(Baseline::Sub))
            .mark_at("2 stehen", 1, Mark::Baseline(Baseline::Super)),
        Para::new("Lage: HHHH normal, HHHH hoch, HHHH tief.", BODY)
            .mark_at("HHHH hoch", 4, Mark::Baseline(Baseline::Super))
            .mark_at("HHHH tief", 4, Mark::Baseline(Baseline::Sub)),
        Para::new(format!("Links: {long}"), BODY),
        Para::new(format!("Mitte: {}", filler(8, 9)), BODY).attrs(|a| a.align(ParagraphAlign::Center)),
        Para::new(format!("Rechts: {}", filler(9, 9)), BODY).attrs(|a| a.align(ParagraphAlign::Right)),
        Para::new(format!("Block: {long}"), BODY).attrs(|a| a.align(ParagraphAlign::Justified)),
        Para::new(format!("Einzug: {long}"), BODY).attrs(|a| a.indent(36.0, 18.0)),
        Para::new(format!("Hängend: {long}"), BODY).attrs(|a| a.indent(36.0, -18.0)),
        Para::new("Punkt eins, in der Überschriftenschrift", HEADING).block(Block::Bullet(0)),
        Para::new("Punkt zwei, eingerückt", BODY).block(Block::Bullet(1)).attrs(|a| a.indent(36.0, 0.0)),
        Para::new("Erstens", BODY).block(Block::Numbered(0)),
        Para::new("Zweitens", BODY).block(Block::Numbered(0)),
        Para::new(format!("Teilen: {}", filler(10, 20)), HEADING).attrs(|a| a.align(ParagraphAlign::Center).indent(12.0, 6.0).page_break_before(true)),
        Para::new("Danach: ein gewöhnlicher Absatz.", BODY),
    ])
}

/// Locked atoms between paragraphs.
pub fn atoms() -> Doc {
    Doc::of(vec![
        Para::new("Gesperrte Blöcke", TITLE),
        Para::new("Davor steht ein Absatz.", BODY),
        Para::new(ATOM, BODY).attrs(|a| a.atom(ATOM_HEIGHT)),
        Para::new("Danach steht ein Absatz.", BODY),
        Para::new("Bild – in dieser Version nicht bearbeitbar", BODY).attrs(|a| a.atom(60.0)),
        Para::new("Am Ende steht ein Absatz.", BODY),
    ])
}

/// The page rules: `fill` one-line paragraphs with no room around them (a page holds 50),
/// then `last`. `rules:<fill>:<last>` with `last` one of:
/// - `para`: a body paragraph of several lines, the document's last;
/// - `head`: a heading with room above it, flowing on from the lines before;
/// - `break`: the same heading after a forced page break.
pub fn rules(fill: usize, last: &str) -> Doc {
    let mut paras: Vec<Para> = (1..=fill).map(|i| Para::new(format!("Zeile {i}"), TIGHT)).collect();
    paras.push(match last {
        "head" => Para::new("Ziel: eine Überschrift", HEADING),
        "break" => Para::new("Ziel: eine Überschrift", HEADING).attrs(|a| a.page_break_before(true)),
        _ => Para::new(format!("Ziel: {}", filler(11, 58)), TIGHT),
    });
    Doc::of(paras)
}

/// The document `PAGED_DOC` names, the generated one of `PAGED_WORDS` words without it.
pub fn from_env() -> Doc {
    let doc = std::env::var("PAGED_DOC").unwrap_or_default();
    let mut parts = doc.split(':');
    match parts.next() {
        Some("features") => features(),
        Some("atoms") => atoms(),
        Some("rules") => {
            let fill = parts.next().and_then(|n| n.parse().ok()).unwrap_or(0);
            rules(fill, parts.next().unwrap_or("para"))
        }
        _ => generate(std::env::var("PAGED_WORDS").ok().and_then(|w| w.parse().ok()).unwrap_or(25_000)),
    }
}
