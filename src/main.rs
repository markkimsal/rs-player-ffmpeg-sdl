use crate::app::{open_input, open_window};

mod app;
mod filter;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    // you can't pass cli arguments to debug with rust-analyzer
    let defailt_file = String::from("foo.mp4");
    let (_, format_context, codec_context) = open_input(args.get(1).unwrap_or(&defailt_file));
    open_window(format_context, codec_context);
}
