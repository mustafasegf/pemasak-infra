use std::io::Write;

fn main() {
    println!("cargo:rerun-if-changed=ui");
    let output = std::process::Command::new("pnpm")
        .current_dir("ui")
        .arg("build")
        .output()
        .expect("failed to execute process");

    std::io::stdout().write_all(&output.stdout).unwrap();
    std::io::stderr().write_all(&output.stderr).unwrap();

    if !output.status.success() {
        panic!("failed to build ui");
    }
}
