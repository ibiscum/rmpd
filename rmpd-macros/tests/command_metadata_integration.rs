use rmpd_macros::CommandMetadata;

#[allow(dead_code)]
#[derive(CommandMetadata)]
enum TestCommand {
    #[command(name = "play", permission = 4)]
    Play { position: Option<u32> },
    #[command(name = "stop")]
    Stop,
    #[command(name = "seekcur", permission = 2)]
    SeekCur(u32),
}

#[test]
fn derives_name_and_permission_for_all_variant_shapes() {
    let play = TestCommand::Play { position: Some(42) };
    assert_eq!(play.command_name(), "play");
    assert_eq!(play.command_required_permission(), 4);

    let stop = TestCommand::Stop;
    assert_eq!(stop.command_name(), "stop");
    assert_eq!(stop.command_required_permission(), 0);

    let seek = TestCommand::SeekCur(10);
    assert_eq!(seek.command_name(), "seekcur");
    assert_eq!(seek.command_required_permission(), 2);
}
