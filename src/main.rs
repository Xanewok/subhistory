use std::io::BufRead;
use std::io::Write;
use std::process::Command;

const RUST_REPO_PATH: &str = "/home/xanewok/repos/rust";
const RLS_REPO_PATH: &str = "/home/xanewok/repos/rls";

fn read_line(read: &mut impl BufRead) -> Option<String> {
    let mut line = String::new();
    read.read_line(&mut line)
        .ok()
        .and_then(|read_bytes| if read_bytes == 0 { None } else { Some(line) })
}

fn main() {
    let stdin = std::io::stdin();
    let stdout = std::io::stdout();
    let mut stdin = stdin.lock();
    let mut stdout = stdout.lock();

    // Collect existing Rust tags into array of [(ISO date, tag name)]
    let rust_tags: Vec<(String, String)> = String::from_utf8(
        Command::new("git")
            .args(&[
                "tag",
                "-l",
                "--sort=creatordate",
                "--format=%(creatordate:iso-strict)|%(refname:short)",
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

    let mut orphan_ranges = vec![];
    // Read every two lines - first should be Rust commit with submodule bump
    // The second line should specify RLS commit range "start...end"
    while let (Some(rust_commit_hash), Some(rls_commit_range)) =
        (read_line(&mut stdin), read_line(&mut stdin))
    {
        let rust_commit_hash = rust_commit_hash.trim();
        let rls_commit_range = match &rls_commit_range.trim()["Submodule src/tools/rls ".len()..] {
            range if range.ends_with(':') => &range[..range.len() - ":".len()],
            range if range.ends_with(" (new submodule)") => {
                &range[..range.len() - " (new submodule)".len()]
            }
            range if range.ends_with(" (commits not present)") => {
                &range[..range.len() - " (commits not present)".len()]
            }
            range => range,
        };

        let parent_details = String::from_utf8(
            Command::new("git")
                .args(&["log", "-n", "1", "--pretty=%cI%x09%H", rust_commit_hash])
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
        // Most recent Rust tag release for a given "parent" (Rust repo) commit
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

        let mut orphans = vec![];
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

            // Assuming RLS repo has `upstream` remote set to rust-lang/rls, check
            // if the children commit in range is contained in the master history
            // (is an ancestor)
            let is_ancestor = Command::new("git")
                .args(&["merge-base", "--is-ancestor", &commit, "upstream/master"])
                .current_dir(RLS_REPO_PATH)
                .output()
                .unwrap()
                .status
                .success();
            let status_char = if is_ancestor { '✓' } else { '❌' };
            if !is_ancestor {
                orphans.push(commit);
            }

            let _ = writeln!(&mut stdout, "  ({}) {}", status_char, details.trim());
        }

        if !orphans.is_empty() {
            orphan_ranges.push((rust_commit_hash.to_owned(), orphans));
        }
    }

    dbg!(&orphan_ranges);
}
