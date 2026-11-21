//! Gherkin (`.feature`) colors for `code_view`: the keywords of the blocks, the steps, the tags, the
//! comments, the doc strings, the tables and the parameters of an outline, in English and French.

use egui;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gherkin {
    Plain,
    Keyword,
    Step,
    Tag,
    Comment,
    Text,
    Param,
    Pipe,
    Doc,
}

/// The keywords that open a block (followed by a colon), longest first
const BLOCKS: [&str; 20] = [
    "Scenario Outline",
    "Scenario Template",
    "Scenarios",
    "Scenario",
    "Business Need",
    "Background",
    "Examples",
    "Example",
    "Feature",
    "Ability",
    "Rule",
    "Plan du scénario",
    "Plan du Scénario",
    "Fonctionnalité",
    "Scénario",
    "Contexte",
    "Exemples",
    "Exemple",
    "Règle",
    "Besoin métier",
];

/// The keywords of the steps, with what follows them, longest first
const STEPS: [&str; 20] = [
    "Étant donné que ",
    "Étant donné qu'",
    "Etant donné que ",
    "Étant donné ",
    "Etant donné ",
    "Given ",
    "When ",
    "Then ",
    "And ",
    "But ",
    "Soit ",
    "Quand ",
    "Lorsque ",
    "Lorsqu'",
    "Alors ",
    "Donc ",
    "Et que ",
    "Et ",
    "Mais ",
    "* ",
];

/// The text of a step or of a cell cut in (start, end, kind) pieces: "quoted text" and <parameters>
fn inline(s: &str) -> Vec<(usize, usize, Gherkin)> {
    let mut out = Vec::new();
    let mut at = 0;
    let mut i = 0;
    let bytes = s.as_bytes();
    while i < bytes.len() {
        let close = match bytes[i] {
            b'"' => s[i + 1..].find('"').map(|e| (i + 1 + e + 1, Gherkin::Text)),
            b'<' => s[i + 1..]
                .find('>')
                .filter(|e| *e > 0 && *e <= 40 && !s[i + 1..i + 1 + e].contains(['<', '"']))
                .map(|e| (i + 1 + e + 1, Gherkin::Param)),
            _ => None,
        };
        match close {
            Some((end, kind)) => {
                if i > at {
                    out.push((at, i, Gherkin::Plain));
                }
                out.push((i, end, kind));
                i = end;
                at = end;
            }
            None => i += s[i..].chars().next().map_or(1, char::len_utf8),
        }
    }
    if at < s.len() {
        out.push((at, s.len(), Gherkin::Plain));
    }
    out
}

/// One line of Gherkin cut in (start, end, kind) pieces that cover it. `doc` is the delimiter of the
/// doc string the line is in (three double quotes or three backticks), kept from a line to the next
pub fn line_pieces(line: &str, doc: &mut Option<&'static str>) -> Vec<(usize, usize, Gherkin)> {
    let indent = line.len() - line.trim_start().len();
    let body = &line[indent..];
    let mut out = Vec::new();
    if indent > 0 {
        out.push((0, indent, Gherkin::Plain));
    }
    let mut put = |a: usize, z: usize, k: Gherkin| {
        if z > a {
            out.push((indent + a, indent + z, k));
        }
    };
    let delimiter = ["\"\"\"", "```"].into_iter().find(|d| body.starts_with(d));
    if let Some(open) = *doc {
        put(0, body.len(), Gherkin::Doc);
        if body.starts_with(open) {
            *doc = None;
        }
    } else if let Some(d) = delimiter {
        *doc = Some(d);
        put(0, body.len(), Gherkin::Doc);
    } else if body.starts_with('#') {
        put(0, body.len(), Gherkin::Comment);
    } else if body.starts_with('@') {
        // the tags are the words that start with @
        let mut at = 0;
        for word in body.split_inclusive(char::is_whitespace) {
            let kind = if word.starts_with('@') { Gherkin::Tag } else { Gherkin::Plain };
            put(at, at + word.len(), kind);
            at += word.len();
        }
    } else if body.starts_with('|') {
        // a row of a table: the pipes, and the cells with their text and parameters
        let mut at = 0;
        for (n, cell) in body.split('|').enumerate() {
            if n > 0 {
                put(at, at + 1, Gherkin::Pipe);
                at += 1;
            }
            for (a, z, k) in inline(cell) {
                put(at + a, at + z, k);
            }
            at += cell.len();
        }
    } else if let Some(word) = BLOCKS.iter().find(|w| body.starts_with(*w) && body[w.len()..].starts_with(':')) {
        put(0, word.len() + 1, Gherkin::Keyword);
        for (a, z, k) in inline(&body[word.len() + 1..]) {
            put(word.len() + 1 + a, word.len() + 1 + z, k);
        }
    } else if let Some(word) = STEPS.iter().find(|w| body.starts_with(*w)) {
        put(0, word.len(), Gherkin::Step);
        for (a, z, k) in inline(&body[word.len()..]) {
            put(word.len() + a, word.len() + z, k);
        }
    } else {
        for (a, z, k) in inline(body) {
            put(a, z, k);
        }
    }
    out
}

/// Colors a Gherkin source
pub fn job(ui: &egui::Ui, text: &str) -> egui::text::LayoutJob {
    use egui::{text::LayoutJob, Color32, TextFormat};
    let dark = ui.visuals().dark_mode;
    let pick = |d: (u8, u8, u8), l: (u8, u8, u8)| {
        let c = if dark { d } else { l };
        Color32::from_rgb(c.0, c.1, c.2)
    };
    let color = |k: Gherkin| match k {
        Gherkin::Plain => ui.visuals().text_color(),
        Gherkin::Keyword => pick((235, 180, 90), (150, 90, 0)),
        Gherkin::Step => pick((200, 130, 225), (130, 40, 160)),
        Gherkin::Tag => pick((100, 180, 230), (0, 100, 160)),
        Gherkin::Comment => ui.visuals().weak_text_color(),
        Gherkin::Text => pick((150, 200, 120), (40, 120, 40)),
        Gherkin::Param => pick((240, 160, 120), (170, 60, 20)),
        Gherkin::Pipe => pick((130, 200, 200), (0, 110, 110)),
        Gherkin::Doc => pick((150, 200, 120), (40, 120, 40)),
    };
    let font = egui::TextStyle::Monospace.resolve(ui.style());
    let mut job = LayoutJob::default();
    let mut doc = None;
    let lines: Vec<&str> = text.split('\n').collect();
    for (n, full) in lines.iter().enumerate() {
        let line = full.trim_end_matches('\r');
        for (a, z, k) in line_pieces(line, &mut doc) {
            job.append(&line[a..z], 0.0, TextFormat::simple(font.clone(), color(k)));
        }
        job.append(&full[line.len()..], 0.0, TextFormat::simple(font.clone(), color(Gherkin::Plain)));
        if n + 1 < lines.len() {
            job.append("\n", 0.0, TextFormat::simple(font.clone(), color(Gherkin::Plain)));
        }
    }
    job
}

#[cfg(test)]
mod tests {
    use super::*;

    fn kinds(line: &str) -> Vec<(String, Gherkin)> {
        let mut doc = None;
        line_pieces(line, &mut doc).into_iter().map(|(a, z, k)| (line[a..z].to_string(), k)).collect()
    }

    #[test]
    fn a_line_is_covered_without_a_hole() {
        for line in [
            "",
            "  Feature: Login <x> \"é\" ",
            "| a | <b> | \"c\" |",
            "@a @b rest",
            "Étant donné que é",
            "<",
            "\"",
            "|",
            "# é",
            "    ",
            "Given <>",
            "Given <é",
        ] {
            let mut doc = None;
            let joined: String = line_pieces(line, &mut doc).iter().map(|(a, z, _)| &line[*a..*z]).collect();
            assert_eq!(joined, line);
        }
    }

    #[test]
    fn blocks_steps_tags_and_comments() {
        assert_eq!(kinds("Feature: Login")[0], ("Feature:".to_string(), Gherkin::Keyword));
        assert_eq!(kinds("  Scenario Outline: x")[1], ("Scenario Outline:".to_string(), Gherkin::Keyword));
        assert_eq!(kinds("Fonctionnalité: x")[0].1, Gherkin::Keyword);
        assert_eq!(kinds("    Given a user")[1], ("Given ".to_string(), Gherkin::Step));
        assert_eq!(kinds("Étant donné que x")[0], ("Étant donné que ".to_string(), Gherkin::Step));
        assert_eq!(kinds("Andrew is here")[0].1, Gherkin::Plain);
        assert_eq!(kinds("# a comment")[0].1, Gherkin::Comment);
        let tags = kinds("@smoke @slow");
        assert_eq!(tags.iter().filter(|t| t.1 == Gherkin::Tag).count(), 2);
    }

    #[test]
    fn text_parameters_tables_and_doc_strings() {
        let step = kinds("When I enter \"bob\" as <name>");
        assert!(step.contains(&("\"bob\"".to_string(), Gherkin::Text)));
        assert!(step.contains(&("<name>".to_string(), Gherkin::Param)));
        let row = kinds("| a | <b> |");
        assert_eq!(row.iter().filter(|t| t.1 == Gherkin::Pipe).count(), 3);
        assert!(row.contains(&("<b>".to_string(), Gherkin::Param)));
        let mut doc = None;
        line_pieces("\"\"\"json", &mut doc);
        assert!(doc.is_some());
        let inside = line_pieces("Given not a step", &mut doc);
        assert!(inside.iter().all(|s| s.2 == Gherkin::Doc));
        line_pieces("\"\"\"", &mut doc);
        assert!(doc.is_none());
    }
}
