use std::path::Path;

use super::command;

#[test]
fn the_command_passes_the_path_to_the_os_handler_without_running_it() {
    let path = Path::new("/tmp/a dir/config.toml");
    let command = command(path);
    let program = command.get_program().to_string_lossy().into_owned();
    let args: Vec<String> = command
        .get_args()
        .map(|arg| arg.to_string_lossy().into_owned())
        .collect();
    if cfg!(target_os = "macos") {
        assert_eq!(program, "open");
    } else if cfg!(target_os = "windows") {
        assert_eq!(program, "cmd");
    } else {
        assert_eq!(program, "xdg-open");
    }
    assert_eq!(
        args.last().map(String::as_str),
        Some("/tmp/a dir/config.toml"),
        "the path is one argument, never split"
    );
}
