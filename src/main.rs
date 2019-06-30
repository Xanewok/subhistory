use std::io::BufRead;
use std::io::Write;
use std::process::Command;

const RUST_REPO_PATH: &str = "/home/xanewok/repos/rust";
const RLS_REPO_PATH: &str = "/home/xanewok/repos/rls";

fn main() {
    let stdin = std::io::stdin();
    let mut stdin = stdin.lock();
    let stdout = std::io::stdout();
    let mut stdout = stdout.lock();

    let mut read_two_lines = || {
        (0..2)
            .map(|_| {
                let mut line = String::new();
                let read = stdin.read_line(&mut line).ok();
                read.and_then(|read| if read == 0 { None } else { Some(line) })
            })
            .collect::<Option<Vec<String>>>()
            .and_then(|lines| if lines.len() < 2 { None } else { Some(lines) })
    };

    let rust_tags: Vec<(String, String)> = String::from_utf8(
        Command::new("git")
            .args(&[
                "tag",
                "-l",
                "--sort=creatordate",
                "--format=%(creatordate:iso)|%(refname:short)",
            ])
            .current_dir(RUST_REPO_PATH)
            .output()
            .unwrap()
            .stdout,
    )
    .unwrap()
    .lines()
    .map(|line| {
        let pipe = line.find('|').unwrap();
        let (date, name) = line.split_at(pipe);

        (date.to_owned(), name[1..].to_owned())
    })
    .collect();

    while let Some(lines) = read_two_lines() {
        let rust_commit_hash = lines[0].trim();
        let rls_commit_range = match &lines[1].trim()["Submodule src/tools/rls ".len()..] {
            range if range.ends_with(':') => &range[..range.len() - ":".len()],
            range if range.ends_with(" (new submodule)") => {
                &range[..range.len() - " (new submodule)".len()]
            }
            range if range.ends_with(" (commits not present)") => {
                &range[..range.len() - " (commits not present)".len()]
            }
            range => range,
        };
        dbg!(&rls_commit_range);

        let parent_details = String::from_utf8(
            Command::new("git")
                .args(&["log", "-n", "1", "--pretty=%ci%x09%H", rust_commit_hash])
                .current_dir(RUST_REPO_PATH)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let children_details = String::from_utf8(
            Command::new("git")
                .args(&["log", "--pretty=%H", rls_commit_range])
                .current_dir(RLS_REPO_PATH)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();
        let children_commits: Vec<_> = children_details.lines().map(ToOwned::to_owned).collect();

        let parent_date = parent_details.split_whitespace().nth(0).unwrap();
        let release_idx = match rust_tags.binary_search_by_key(&parent_date, |(date, _)| date) {
            Ok(idx) => idx,
            Err(idx) => idx,
        };
        let parent_release = rust_tags
            .get(release_idx - 1)
            .map(|(_, b)| b.as_ref())
            .unwrap_or("None");

        let _ = writeln!(
            &mut stdout,
            "({}) {}",
            parent_release,
            parent_details.trim(),
        );

        for commit in children_commits {
            let details = String::from_utf8(
                Command::new("git")
                    .args(&["log", "-n", "1", "--pretty=%ci%x09%H%x09%s", &commit])
                    .current_dir(RLS_REPO_PATH)
                    .output()
                    .unwrap()
                    .stdout,
            )
            .unwrap();

            let is_ancestor = Command::new("git")
                .args(&["merge-base", "--is-ancestor", &commit, "upstream/master"])
                .current_dir(RLS_REPO_PATH)
                .output()
                .unwrap()
                .status
                .success();
            let status_char = if is_ancestor { '✓' } else { '❌' };

            let _ = writeln!(&mut stdout, "  ({}) {}", status_char, details.trim());
        }
    }
}
