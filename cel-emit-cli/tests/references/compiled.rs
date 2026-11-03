//! A generated program or a reference, run once on stdin's JSON in two counted phases: `decode`,
//! the text to the program's input, then `expression`, that input to the output text. The harness
//! adds after this the `program` module and a `use` naming its two functions `decode` and
//! `expression`.

mod count;

use std::fmt::Debug;
use std::hint::black_box;

#[global_allocator]
static COUNTING: count::Counting = count::Counting;

/// `text` decoded by `decode`.
#[inline(never)]
fn measured_decode<T, E: Debug>(decode: fn(&str) -> Result<T, E>, text: &str) -> Result<T, String> {
    decode(text).map_err(|e| format!("{e:?}"))
}

/// `input`'s output text, by `expression`.
#[inline(never)]
fn measured_expression<T, E: Debug>(
    expression: fn(&T) -> Result<String, E>,
    input: &T,
) -> Result<String, String> {
    expression(input).map_err(|e| format!("{e:?}"))
}

fn main() {
    let text = std::io::read_to_string(std::io::stdin()).unwrap();
    let out =
        count::counted("decode", || measured_decode(decode, black_box(&text))).and_then(|input| {
            count::counted("expression", || {
                measured_expression(expression, black_box(&input))
            })
        });
    match out {
        Ok(out) => println!("{out}"),
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    }
}
