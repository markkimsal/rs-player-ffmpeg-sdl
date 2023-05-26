use crate::app::{open_input, open_window};

mod app;

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let (_, format_context, codec_context) = open_input(args.get(1).unwrap());
    open_window(format_context, codec_context);
}
