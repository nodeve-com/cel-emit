//! A program's Rust source, measured whole and, for a generated program, split by what each part
//! serves.

/// Lines and bytes.
pub(super) type Size = [usize; 2];

/// One program's source. A generated program's splits into its decode, and its expression with the
/// functions that calls; the rest, the `use`, `output`, and `main`, is in neither.
pub(super) struct Source {
    pub(super) whole: Size,
    pub(super) decode: Option<Size>,
    pub(super) expression: Option<Size>,
}

fn size(text: &str) -> Size {
    [text.lines().count(), text.len()]
}

/// `text`, a reference's or any program's source, measured whole only.
pub(super) fn whole(text: &str) -> Source {
    Source {
        whole: size(text),
        decode: None,
        expression: None,
    }
}

/// `text`, a generated program's source, measured whole and by part: the types and `decode`
/// function are the decode, `use`, `output`, and `main` are neither, the rest the expression.
pub(super) fn split(text: &str) -> Source {
    let file = syn::parse_file(text).expect("a generated program parses");
    let (mut decode, mut expression) = (String::new(), String::new());
    for item in file.items {
        let name = match &item {
            syn::Item::Mod(m) => m.ident.to_string(),
            syn::Item::Fn(f) => f.sig.ident.to_string(),
            syn::Item::Use(_) => continue,
            _ => String::new(),
        };
        let part = match name.as_str() {
            "types" | "decode" => &mut decode,
            "output" | "main" => continue,
            _ => &mut expression,
        };
        let one = syn::File {
            shebang: None,
            attrs: Vec::new(),
            items: vec![item],
        };
        part.push_str(&prettyplease::unparse(&one));
    }
    Source {
        whole: size(text),
        decode: Some(size(&decode)),
        expression: Some(size(&expression)),
    }
}
