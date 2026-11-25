//! PDF: what a file says about itself (`pdf_info`), read with `craft-pdf` (hayro), without Python.
//!
//! The file is read and parsed with the GIL released, so a big PDF read in a thread does not make the
//! window stutter (pypdf, plain Python, did). Nothing is raised for a bad file: the reason is in the
//! `error` key of the answer.

use std::sync::Arc;

use craft_pdf::syntax::object::{Dict, ObjectIdentifier};
use craft_pdf::syntax::{DecodeLimits, LoadPdfError, Pdf, PdfVersion};

/// Longest value of the information dictionary, in characters.
const MAX_VALUE_CHARS: usize = 1000;
/// Largest decoded stream accepted while opening (a bomb is refused, not expanded).
const MAX_DECODED_STREAM: usize = 64 * 1024 * 1024;
/// Largest file read, in bytes.
const MAX_INPUT_BYTES: usize = 512 * 1024 * 1024;
/// Bytes looked at at each end of the file for the `/Encrypt` key of the trailer.
const ENCRYPT_SCAN_BYTES: usize = 16 * 1024;

/// Permission bits (1-based, ISO 32000 table 22) behind the `can_*` keys.
pub const PERMISSIONS: [(&str, u32); 6] = [
    ("can_print", 3),
    ("can_copy", 5),
    ("can_modify", 4),
    ("can_annotate", 6),
    ("can_fill_forms", 9),
    ("can_assemble", 11),
];

/// What a document says about itself, with the keys of the Python answer.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Info {
    pub pages: Option<usize>,
    pub encrypted: bool,
    pub needs_password: bool,
    /// The `/P` entry of the encryption dictionary, when the file is encrypted and it could be read.
    pub permissions: Option<i32>,
    pub title: Option<String>,
    pub author: Option<String>,
    pub subject: Option<String>,
    pub keywords: Option<String>,
    pub creator: Option<String>,
    pub producer: Option<String>,
    /// Seconds since the epoch.
    pub created: Option<f64>,
    pub modified: Option<f64>,
    pub pdf_version: Option<String>,
    pub error: Option<String>,
}

fn skip_space(b: &[u8], mut i: usize) -> usize {
    while b.get(i).is_some_and(u8::is_ascii_whitespace) {
        i += 1;
    }
    i
}

fn read_uint(b: &[u8], i: usize) -> Option<(u64, usize)> {
    let end = i + b.get(i..)?.iter().take_while(|c| c.is_ascii_digit()).count();
    let n = std::str::from_utf8(b.get(i..end)?).ok()?.parse().ok()?;
    (end > i).then_some((n, end))
}

/// The value of `/P` in the encryption dictionary named by the trailer, read from the raw bytes: the
/// dictionary is never inside an object stream and is not itself encrypted, so no parser (and no
/// password) is needed. `None` when it is not found.
pub fn read_permissions(bytes: &[u8]) -> Option<i32> {
    const KEY: &[u8] = b"/Encrypt";
    let at = bytes.windows(KEY.len() + 1).rposition(|w| w.starts_with(KEY) && !w[KEY.len()].is_ascii_alphabetic())?;
    let after = skip_space(bytes, at + KEY.len());
    let dict = if bytes.get(after..after + 2) == Some(b"<<") {
        after
    } else {
        let (num, i) = read_uint(bytes, after)?;
        let (generation, i) = read_uint(bytes, skip_space(bytes, i))?;
        let i = skip_space(bytes, i);
        if bytes.get(i) != Some(&b'R') {
            return None;
        }
        let head = format!("{num} {generation} obj");
        let found = bytes
            .windows(head.len())
            .enumerate()
            .filter(|(_, w)| *w == head.as_bytes())
            .map(|(p, _)| p)
            .rfind(|&p| p == 0 || !bytes[p - 1].is_ascii_digit())?;
        skip_space(bytes, found + head.len())
    };
    // the dictionary is small; its `/P` is at nesting depth 1
    let body = bytes.get(dict..(dict + 4096).min(bytes.len()))?;
    let mut depth = 0i32;
    let mut i = 0;
    while i + 1 < body.len() {
        if body[i] == b'<' && body[i + 1] == b'<' {
            depth += 1;
            i += 2;
        } else if body[i] == b'>' && body[i + 1] == b'>' {
            depth -= 1;
            if depth <= 0 {
                return None;
            }
            i += 2;
        } else if depth == 1 && body[i] == b'/' && body[i + 1] == b'P' && !body.get(i + 2).is_some_and(u8::is_ascii_alphabetic) {
            let j = skip_space(body, i + 2);
            let (neg, j) = if body.get(j) == Some(&b'-') { (true, j + 1) } else { (false, j) };
            let (n, _) = read_uint(body, j)?;
            let n = i64::try_from(n).ok()?;
            let n = if neg { -n } else { n };
            // some writers give the unsigned form of the 32-bit value
            return i32::try_from(n).ok().or_else(|| u32::try_from(n).ok().map(|u| u as i32));
        } else {
            i += 1;
        }
    }
    None
}

/// Whether the head or the tail of the file names an `/Encrypt` dictionary (the key of the trailer).
pub fn names_encryption(bytes: &[u8]) -> bool {
    const KEY: &[u8] = b"/Encrypt";
    let found = |part: &[u8]| part.windows(KEY.len() + 1).any(|w| w.starts_with(KEY) && w.last().is_some_and(|next| !next.is_ascii_alphabetic()));
    found(bytes.get(..ENCRYPT_SCAN_BYTES.min(bytes.len())).unwrap_or_default())
        || found(bytes.get(bytes.len().saturating_sub(ENCRYPT_SCAN_BYTES)..).unwrap_or_default())
}

/// PDFDocEncoding is Latin-1 for the text we meet; UTF-16BE with a BOM and UTF-8 are the others.
/// Control characters become spaces, the text is cut and trimmed; `None` when nothing is left.
pub fn decode_text(bytes: &[u8]) -> Option<String> {
    let text = if let Some(rest) = bytes.strip_prefix(&[0xFE, 0xFF]) {
        let units: Vec<u16> = rest.chunks_exact(2).map(|p| u16::from_be_bytes([p[0], p[1]])).collect();
        String::from_utf16_lossy(&units)
    } else if let Some(rest) = bytes.strip_prefix(&[0xEF, 0xBB, 0xBF]) {
        String::from_utf8_lossy(rest).into_owned()
    } else {
        match std::str::from_utf8(bytes) {
            Ok(s) => s.to_owned(),
            Err(_) => bytes.iter().map(|&b| char::from(b)).collect(),
        }
    };
    let text: String = text.chars().map(|c| if c.is_control() { ' ' } else { c }).take(MAX_VALUE_CHARS).collect();
    let text = text.trim();
    (!text.is_empty()).then(|| text.to_owned())
}

/// Seconds since the epoch of a date and its offset from UTC in seconds, or the time zone of the machine
/// when the date gives none; `None` when it is not a real date.
fn epoch(year: i32, month: u32, day: u32, hour: u32, minute: u32, second: u32, offset: Option<i64>) -> Option<f64> {
    use chrono::TimeZone;
    let date = chrono::NaiveDate::from_ymd_opt(year, month.max(1), day.max(1))?;
    let local = date.and_hms_opt(hour, minute, second)?;
    let seconds = match offset {
        Some(offset) => local.and_utc().timestamp() - offset,
        None => chrono::Local.from_local_datetime(&local).earliest()?.timestamp(),
    };
    Some(seconds as f64)
}

/// A date of the metadata as seconds since the epoch; `None` when it is not a real date.
fn timestamp(d: &craft_pdf::syntax::object::DateTime) -> Option<f64> {
    let minutes = i64::from(d.utc_offset_minute) * if d.utc_offset_hour < 0 { -1 } else { 1 };
    let offset = i64::from(d.utc_offset_hour) * 3600 + minutes * 60;
    epoch(i32::from(d.year), u32::from(d.month), u32::from(d.day), u32::from(d.hour), u32::from(d.minute), u32::from(d.second), Some(offset))
}

/// A date written as `D:YYYYMMDDHHmmSSOHH'mm'`, with the `D:` and everything after the year optional:
/// many writers leave the `D:` out and the viewers read it all the same (hayro does not).
fn parse_date(text: &[u8]) -> Option<f64> {
    let text = text.strip_prefix(b"D:").unwrap_or(text);
    let digits = text.iter().take_while(|c| c.is_ascii_digit()).count().min(14);
    let number = |from: usize, len: usize, default: u32| -> Option<u32> {
        if digits < from + len {
            return Some(default);
        }
        std::str::from_utf8(&text[from..from + len]).ok()?.parse().ok()
    };
    if digits < 4 {
        return None;
    }
    let year = i32::try_from(number(0, 4, 0)?).ok()?;
    let (month, day) = (number(4, 2, 1)?, number(6, 2, 1)?);
    let (hour, minute, second) = (number(8, 2, 0)?, number(10, 2, 0)?, number(12, 2, 0)?);
    let offset = match text.get(digits) {
        Some(b'Z') => Some(0),
        Some(sign @ (b'+' | b'-')) => {
            let rest: Vec<u8> = text[digits + 1..].iter().copied().filter(u8::is_ascii_digit).collect();
            let hh: i64 = std::str::from_utf8(rest.get(..2)?).ok()?.parse().ok()?;
            let mm: i64 = rest.get(2..4).and_then(|m| std::str::from_utf8(m).ok()?.parse().ok()).unwrap_or(0);
            Some((hh * 3600 + mm * 60) * if *sign == b'-' { -1 } else { 1 })
        }
        _ => None,
    };
    epoch(year, month, day, hour, minute, second, offset)
}

/// The object of the information dictionary: the last `/Info N G R` of the file (a trailer, or the
/// dictionary of an xref stream, which are never compressed).
fn find_info_ref(bytes: &[u8]) -> Option<ObjectIdentifier> {
    const KEY: &[u8] = b"/Info";
    let at = bytes.windows(KEY.len() + 1).rposition(|w| w.starts_with(KEY) && !w[KEY.len()].is_ascii_alphabetic())?;
    let (num, i) = read_uint(bytes, skip_space(bytes, at + KEY.len()))?;
    let (generation, i) = read_uint(bytes, skip_space(bytes, i))?;
    (bytes.get(skip_space(bytes, i)) == Some(&b'R')).then_some(())?;
    Some(ObjectIdentifier::new(i32::try_from(num).ok()?, i32::try_from(generation).ok()?))
}

/// A date of the information dictionary read by hand, for what hayro does not read (a date without the
/// `D:`, which many writers leave out and every viewer reads). The dictionary is resolved by hayro, so
/// it may be in an object stream or encrypted.
fn info_date(pdf: &Pdf, bytes: &[u8], key: &[u8]) -> Option<f64> {
    let dict = pdf.xref().get::<Dict<'_>>(find_info_ref(bytes)?)?;
    parse_date(&dict.get::<craft_pdf::syntax::object::String<'_>>(key)?)
}

fn version_text(v: &PdfVersion) -> String {
    // The variants are named `Pdf17`: "Pdf" then the two digits of the version.
    let name = format!("{v:?}");
    match name.strip_prefix("Pdf").map(|d| d.chars().collect::<Vec<_>>()) {
        Some(d) if d.len() == 2 => format!("{}.{}", d[0], d[1]),
        _ => name,
    }
}

fn read_open(pdf: &Pdf, bytes: &[u8], info: &mut Info) {
    info.pdf_version = Some(version_text(&pdf.version()));
    info.pages = Some(pdf.pages().len());
    let meta = pdf.metadata();
    info.title = meta.title.as_deref().and_then(decode_text);
    info.author = meta.author.as_deref().and_then(decode_text);
    info.subject = meta.subject.as_deref().and_then(decode_text);
    info.keywords = meta.keywords.as_deref().and_then(decode_text);
    info.creator = meta.creator.as_deref().and_then(decode_text);
    info.producer = meta.producer.as_deref().and_then(decode_text);
    // our reader first: it gives the local time to a date without offset (as the viewers do), hayro gives UTC
    info.created = info_date(pdf, bytes, b"CreationDate").or_else(|| meta.creation_date.as_ref().and_then(timestamp));
    info.modified = info_date(pdf, bytes, b"ModDate").or_else(|| meta.modification_date.as_ref().and_then(timestamp));
}

/// `"1.7"` of the `%PDF-1.7` header, which a catalog may override once the document is open.
fn header_version(bytes: &[u8]) -> Option<String> {
    let head = bytes.get(..bytes.len().min(1024))?;
    let at = head.windows(5).position(|w| w == b"%PDF-")?;
    let version: String = head[at + 5..].iter().take(3).map(|&c| char::from(c)).collect();
    (version.len() == 3 && version.as_bytes()[1] == b'.' && version.as_bytes()[0].is_ascii_digit() && version.as_bytes()[2].is_ascii_digit())
        .then_some(version)
}

/// Opens a PDF held in memory (with `password`, an empty one by default) and tells what it says about
/// itself. The document is given back when it opened. Never fails: what went wrong is in `error`.
pub fn open_with_info(bytes: Arc<Vec<u8>>, password: Option<&str>) -> (Info, Option<Pdf>) {
    let mut info = Info { encrypted: names_encryption(&bytes), ..Info::default() };
    if info.encrypted {
        info.permissions = read_permissions(&bytes);
    }
    let options = craft_pdf::OpenOptions {
        max_input_bytes: MAX_INPUT_BYTES,
        decode_limits: DecodeLimits::default().with_max_decoded_stream(MAX_DECODED_STREAM),
        ..Default::default()
    };
    let mut document = None;
    match craft_pdf::open_with_options(Arc::clone(&bytes), password, &options) {
        Ok(pdf) => {
            // a document that opens with an empty password may still withhold rights (owner password only)
            let read = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| read_open(&pdf, &bytes, &mut info)));
            if read.is_err() {
                info.error = Some("the PDF reader failed on this file".into());
            } else {
                document = Some(pdf);
            }
        }
        Err(craft_pdf::OpenError::Load(LoadPdfError::Decryption(_))) => {
            info.encrypted = true;
            info.needs_password = true;
            // the header is not encrypted
            info.pdf_version = header_version(&bytes);
        }
        Err(craft_pdf::OpenError::Load(LoadPdfError::Invalid)) => info.error = Some("not a readable PDF".into()),
        Err(craft_pdf::OpenError::InputLimit { .. }) => info.error = Some("the PDF is over the size limit".into()),
        Err(craft_pdf::OpenError::Allocation) => info.error = Some("the PDF could not be loaded into memory".into()),
        Err(craft_pdf::OpenError::BackendPanic) => info.error = Some("the PDF reader failed on this file".into()),
        #[allow(unreachable_patterns)]
        Err(other) => info.error = Some(other.to_string()),
    }
    (info, document)
}

/// The information of a PDF given as bytes. Never fails: a file that cannot be read gives `error`.
pub fn info_of_bytes(bytes: Vec<u8>) -> Info {
    open_with_info(Arc::new(bytes), None).0
}

/// The information of the PDF at `path`; the read error becomes `error`.
pub fn info_of_path(path: &str) -> Info {
    let started = std::time::Instant::now();
    let info = match std::fs::read(path) {
        Ok(bytes) => info_of_bytes(bytes),
        // the text of the OS depends on the language and the code page: the kind says it all
        Err(e) => Info { error: Some(format!("cannot read the file ({:?})", e.kind())), ..Info::default() },
    };
    crate::dlog!(
        "pdf",
        "metadata read of {path}: {} in {:.0?}{}",
        info.pages.map_or("? pages".to_string(), |n| format!("{n} pages")),
        started.elapsed(),
        info.error.as_ref().map_or(String::new(), |e| format!(" (error: {e})"))
    );
    info
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn text_is_decoded_from_utf16_utf8_and_latin1_and_controls_become_spaces() {
        assert_eq!(decode_text(&[0xFE, 0xFF, 0x00, b'H', 0x00, 0xE9]).as_deref(), Some("Hé"));
        assert_eq!(decode_text("Œuvre".as_bytes()).as_deref(), Some("Œuvre"));
        assert_eq!(decode_text(&[b'c', b'a', b'f', 0xE9]).as_deref(), Some("café"));
        assert_eq!(decode_text(b"a\tb\nc").as_deref(), Some("a b c"));
        assert_eq!(decode_text(b"  \x00 "), None);
        assert_eq!(decode_text("x".repeat(5000).as_bytes()).unwrap().chars().count(), MAX_VALUE_CHARS);
    }

    #[test]
    fn the_encrypt_key_is_found_at_either_end_and_not_in_a_longer_name() {
        let tail = [vec![b'x'; 40_000], b"trailer<</Size 9/Encrypt 7 0 R>>".to_vec()].concat();
        let head = [b"%PDF-1.6\n<</Encrypt 7 0 R/Filter/Standard>>".to_vec(), vec![b'x'; 40_000]].concat();
        assert!(names_encryption(&tail) && names_encryption(&head));
        let middle = [vec![b'x'; 20_000], b"/Encrypt 1 0 R".to_vec(), vec![b'x'; 20_000]].concat();
        assert!(!names_encryption(b"<</EncryptMetadata false>>") && !names_encryption(&middle));
    }

    #[test]
    fn permissions_are_read_from_a_referenced_or_an_inline_dictionary() {
        let referenced = b"%PDF-1.6\n7 0 obj\n<</Filter/Standard/V 2/R 3/O(x)/U(y)/P -3904>>\nendobj\ntrailer<</Size 9/Encrypt 7 0 R>>".to_vec();
        assert_eq!(read_permissions(&referenced), Some(-3904));
        let inline = b"trailer<</Encrypt<</Filter/Standard/Perms(zz)/P -44/V 5>>>>".to_vec();
        assert_eq!(read_permissions(&inline), Some(-44));
        let unsigned = b"trailer<</Encrypt<</P 4294963392>>>>".to_vec();
        assert_eq!(read_permissions(&unsigned), Some(-3904));
        assert_eq!(read_permissions(b"17 0 obj<</P 5>>endobj trailer<</Encrypt 7 0 R>>"), None, "object 7 is absent");
        assert_eq!(read_permissions(b"no encryption here"), None);
        assert_eq!(read_permissions(b"<</Encrypt<</Perms 3/P 2>>>>"), Some(2), "/Perms is not /P");
    }

    #[test]
    fn a_date_is_seconds_since_the_epoch_with_its_offset() {
        let d = |hour: i8, minute: u8| craft_pdf::syntax::object::DateTime {
            year: 2024, month: 3, day: 5, hour: 12, minute: 0, second: 0, utc_offset_hour: hour, utc_offset_minute: minute,
        };
        let utc = timestamp(&d(0, 0)).unwrap();
        assert_eq!(utc, 1_709_640_000.0);
        assert_eq!(timestamp(&d(2, 0)).unwrap(), utc - 7200.0);
        assert_eq!(timestamp(&d(-5, 30)).unwrap(), utc + 5.0 * 3600.0 + 30.0 * 60.0);
    }

    #[test]
    fn a_date_is_read_with_or_without_the_prefix_and_with_any_precision() {
        let want = 1_709_640_000.0; // 2024-03-05 12:00:00 UTC
        assert_eq!(parse_date(b"D:20240305120000Z"), Some(want));
        assert_eq!(parse_date(b"20240305120000Z"), Some(want), "no D:");
        let local = |h| chrono::Local.with_ymd_and_hms(2024, 3, 5, h, 0, 0).unwrap().timestamp() as f64;
        assert_eq!(parse_date(b"20240305120000"), Some(local(12)), "no offset: the local time");
        assert_eq!(parse_date(b"D:20240305140000+02'00'"), Some(want));
        assert_eq!(parse_date(b"20240305070000-05'00'"), Some(want));
        assert_eq!(parse_date(b"D:20240305120000+02"), Some(want - 7200.0));
        assert_eq!(parse_date(b"D:2024Z"), Some(1_704_067_200.0), "year only: 1 January 00:00");
        assert_eq!(parse_date(b"D:20241Z"), Some(1_704_067_200.0), "a cut field is ignored");
        assert_eq!(parse_date(b"junk"), None);
        assert_eq!(parse_date(b"D:20241399"), None, "month 13");
    }

    #[test]
    fn the_information_dictionary_is_the_last_one_named() {
        let raw = b"trailer<</Size 9/Info 3 0 R>> ... trailer<</Info 12 1 R/Root 1 0 R>>";
        assert_eq!(find_info_ref(raw), Some(ObjectIdentifier::new(12, 1)));
        assert_eq!(find_info_ref(b"<</InfoX 1 0 R>>"), None, "/InfoX is another key");
        assert_eq!(find_info_ref(b"<</Info<</Title(x)>>>>"), None, "a direct dictionary has no number");
    }

    #[test]
    fn the_version_of_the_header_is_read_even_when_the_file_is_encrypted() {
        assert_eq!(header_version(b"%PDF-1.7\n%\xE2\xE3").as_deref(), Some("1.7"));
        assert_eq!(header_version(b"junk\n%PDF-2.0\n").as_deref(), Some("2.0"));
        assert_eq!(header_version(b"%PDF-x.y"), None);
        assert_eq!(header_version(b""), None);
    }

    #[test]
    fn a_file_that_is_not_a_pdf_gives_an_error_and_nothing_else() {
        let info = info_of_bytes(b"hello".to_vec());
        assert!(info.error.is_some() && info.pages.is_none() && !info.needs_password);
        let missing = info_of_path("Z:/does/not/exist.pdf");
        assert!(missing.error.is_some());
    }
}

/// A small PDF built by hand for the tests of the readers: two pages (the first has two lines of text, an
/// internal link, a web link), and one bookmark that goes to the second page.
#[cfg(test)]
pub mod sample {
    pub fn pdf() -> Vec<u8> {
        let stream = |body: &str| format!("<< /Length {} >>\nstream\n{body}\nendstream", body.len());
        let objects = [
            "<< /Type /Catalog /Pages 2 0 R /Outlines 7 0 R >>".to_owned(),
            "<< /Type /Pages /Kids [3 0 R 5 0 R] /Count 2 >>".to_owned(),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 9 0 R >> >> /Annots [10 0 R 11 0 R] >>".to_owned(),
            stream("BT /F1 24 Tf 72 700 Td (Hello world) Tj 0 -40 Td (Second line) Tj ET"),
            "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 6 0 R /Resources << /Font << /F1 9 0 R >> >> >>".to_owned(),
            stream("BT /F1 24 Tf 72 700 Td (Chapter two) Tj ET"),
            "<< /Type /Outlines /First 8 0 R /Last 8 0 R /Count 1 >>".to_owned(),
            "<< /Title (Chapter) /Parent 7 0 R /Dest [5 0 R /XYZ 0 500 0] >>".to_owned(),
            "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_owned(),
            "<< /Type /Annot /Subtype /Link /Rect [72 690 200 720] /Border [0 0 0] /Dest [5 0 R /XYZ 0 500 0] >>".to_owned(),
            "<< /Type /Annot /Subtype /Link /Rect [72 650 200 680] /Border [0 0 0] /A << /S /URI /URI (https://example.com) >> >>".to_owned(),
        ];
        let mut out = b"%PDF-1.4\n".to_vec();
        let mut offsets = Vec::new();
        for (i, object) in objects.iter().enumerate() {
            offsets.push(out.len());
            out.extend_from_slice(format!("{} 0 obj\n{object}\nendobj\n", i + 1).as_bytes());
        }
        let xref = out.len();
        out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
        for offset in offsets {
            out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
        }
        out.extend_from_slice(format!("trailer\n<< /Size {} /Root 1 0 R >>\nstartxref\n{xref}\n%%EOF\n", objects.len() + 1).as_bytes());
        out
    }
}
