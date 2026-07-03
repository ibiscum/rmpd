use rmpd_macros::CommandMetadata;

#[derive(CommandMetadata)]
struct NotAnEnum {
    value: u8,
}

fn main() {}
