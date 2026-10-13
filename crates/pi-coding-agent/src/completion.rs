//! `pi --completion <shell>`: emit a shell completion script.

use std::io::Write;

pub fn print_man() -> Result<(), String> {
    use clap::CommandFactory;
    let cmd = crate::Cli::command();
    let man = clap_mangen::Man::new(cmd);
    let mut so = std::io::stdout().lock();
    man.render(&mut so).map_err(|e| e.to_string())
}

pub fn print_shell(shell: &str) -> Result<(), String> {
    use clap::CommandFactory;
    let mut cmd = crate::Cli::command();
    let out = match shell {
        "bash" => {
            let mut o = Vec::new();
            clap_complete::generate(clap_complete::Shell::Bash, &mut cmd, "pi", &mut o);
            o
        }
        "zsh" => {
            let mut o = Vec::new();
            clap_complete::generate(clap_complete::Shell::Zsh, &mut cmd, "pi", &mut o);
            o
        }
        "fish" => {
            let mut o = Vec::new();
            clap_complete::generate(clap_complete::Shell::Fish, &mut cmd, "pi", &mut o);
            o
        }
        other => return Err(format!("unsupported shell '{other}' (bash|zsh|fish)")),
    };
    let mut so = std::io::stdout().lock();
    let _ = so.write_all(&out);
    let _ = so.flush();
    Ok(())
}
