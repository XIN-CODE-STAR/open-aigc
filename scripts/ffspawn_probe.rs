// 一次性探针：复现 cargo test 环境下 Command::new("ffmpeg") 的解析行为。
// 用后即删，不进提交。
fn main() {
    let cwd = std::env::current_dir().unwrap();
    println!("cwd: {}", cwd.display());
    let exe = std::env::current_exe().unwrap();
    println!("exe: {}", exe.display());
    let s = std::process::Command::new("ffmpeg")
        .arg("-version")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status();
    println!("spawn: {:?}", s);
}
