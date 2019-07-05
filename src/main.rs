use std::collections::BTreeMap;
use std::io::BufRead;
use std::iter::FromIterator;
use std::ops::Range;
use std::path::Path;
use std::process::Command;

trait Single: Iterator {
    fn single(self) -> Option<Self::Item>;
}
impl<I: Iterator> Single for I {
    fn single(mut self) -> Option<Self::Item> {
        match (self.next(), self.next()) {
            (Some(first), None) => Some(first),
            _ => None,
        }
    }
}

fn ancestor_char(is_ancestor: bool) -> char {
    if is_ancestor {
        '✓'
    } else {
        '❌'
    }
}

fn read_from_stdin<'a>(repo_path: &Path) -> impl Iterator<Item = (String, Range<String>)> + 'a {
    let log = Command::new("git")
        .current_dir(repo_path)
        .args(&["log", "--pretty=%H", "-p", "--submodule", "src/tools/rls"])
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();
    let rg = Command::new("rg")
        .current_dir(repo_path)
        .arg("^\\w")
        .stdin(log.stdout.unwrap())
        .stdout(std::process::Stdio::piped())
        .spawn()
        .unwrap();

    let mut input = std::io::BufReader::new(rg.stdout.unwrap());

    std::iter::from_fn(move || {
        let (first, second) = match (read_line(&mut input), read_line(&mut input)) {
            (Some(first), Some(second)) => Some((first, second)),
            _ => None,
        }?;

        let rust_commit_hash = first.trim();
        let rls_commit_range = match &second.trim()["Submodule src/tools/rls ".len()..] {
            range if range.ends_with(':') => &range[..range.len() - ":".len()],
            range if range.ends_with(" (new submodule)") => {
                &range[..range.len() - " (new submodule)".len()]
            }
            range if range.ends_with(" (commits not present)") => {
                &range[..range.len() - " (commits not present)".len()]
            }
            range => range,
        };

        let mut dots = rls_commit_range
            .char_indices()
            .filter(|(_idx, chr)| *chr == '.');
        let (first, last) = (dots.nth(0).unwrap().0, dots.last().unwrap().0);
        let range = (rls_commit_range[..(first - 1)].to_owned())
            ..(rls_commit_range[(last + 1)..].to_owned());

        Some((rust_commit_hash.to_owned(), range))
    })
}

#[allow(unused)]
fn read_from_repository<'a>(
    repo: &'a git2::Repository,
) -> impl 'a + Iterator<Item = (String, Range<String>)> {
    let mut walker = repo.revwalk().expect("Can't create revwalker");
    walker.set_sorting(git2::Sort::TOPOLOGICAL | git2::Sort::TIME);
    walker.push_head().unwrap();

    walker
        .filter_map(move |x| x.and_then(|x| repo.find_commit(x)).ok())
        .filter_map(|x| x.parents().single().map(|parent| (x, parent)))
        .filter_map(move |(commit, parent)| {
            let (commit_tree, parent_tree) = (commit.tree().unwrap(), parent.tree().unwrap());
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.pathspec("src/tools/rls");

            let diff = repo
                .diff_tree_to_tree(Some(&commit_tree), Some(&parent_tree), Some(&mut diff_opts))
                .expect("Can't calculate diffs");

            if diff.deltas().len() == 1 {
                Some((commit, diff))
            } else {
                None
            }
        })
        .map(|(commit, diff)| {
            let delta = diff.deltas().nth(0).unwrap();
            assert_eq!(delta.new_file().path(), Some(Path::new("src/tools/rls")));

            (
                commit.id().to_string(),
                delta.old_file().id().to_string()..delta.new_file().id().to_string(),
            )
        })
}

fn read_line(read: &mut impl BufRead) -> Option<String> {
    let mut line = String::new();
    read.read_line(&mut line)
        .ok()
        .and_then(|read_bytes| if read_bytes == 0 { None } else { Some(line) })
}

#[derive(Clone, Debug)]
struct CommitWithDetails {
    commit: String,
    details: String,
}
#[derive(Clone, Debug)]
struct ChildCommit {
    detailed: CommitWithDetails,
    is_ancestor: bool,
}

#[derive(Debug)]
struct Summary<'a>(&'a BTreeMap<&'a str, Vec<(CommitWithDetails, Vec<ChildCommit>)>>);

impl std::fmt::Display for Summary<'_> {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        for (release, commits) in self.0.iter().rev() {
            for (parent, children) in commits {
                writeln!(f, "({}) {}", release, parent.details)?;
                for child in children {
                    writeln!(
                        f,
                        "  ({}) {}",
                        ancestor_char(child.is_ancestor),
                        child.detailed.details
                    )?;
                }
            }
        }
        Ok(())
    }
}

fn main() {
    let rust_repo_path = std::env::var("RUST_REPO_PATH")
        .unwrap_or_else(|_| String::from("/home/xanewok/repos/rust"));
    let rls_repo_path =
        std::env::var("RLS_REPO_PATH").unwrap_or_else(|_| String::from("/home/xanewok/repos/rls"));

    // let rust_repo = git2::Repository::init(&rust_repo_path).expect("Couldn't init Rust repository");
    // let rls_repo = git2::Repository::init(&rls_repo_path).expect("Couldn't init RLS repository");

    // Collect existing Rust tags into array of [(ISO date, tag name)]
    let rust_tags: Vec<(String, String)> = String::from_utf8(
        Command::new("git")
            .args(&[
                "tag",
                "-l",
                "--sort=creatordate",
                "--format=%(creatordate:iso-strict)|%(refname:short)",
            ])
            .current_dir(&rust_repo_path)
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

    let mut commits: BTreeMap<&str, Vec<(CommitWithDetails, Vec<ChildCommit>)>> = BTreeMap::new();

    let iter = read_from_stdin(Path::new(&rust_repo_path));
    // let iter = read_from_repository(&rust_repo);
    for (rust_commit_hash, rls_commit_range) in iter {
        let parent_details = String::from_utf8(
            Command::new("git")
                .args(&["log", "-n", "1", "--pretty=%cI%x09%H", &rust_commit_hash])
                .current_dir(&rust_repo_path)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();

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

        let range = format!("{}...{}", rls_commit_range.start, rls_commit_range.end);
        let children_details = String::from_utf8(
            Command::new("git")
                .args(&["log", "--pretty=%H", &range, "--left-right"])
                .current_dir(&rls_repo_path)
                .output()
                .unwrap()
                .stdout,
        )
        .unwrap();

        let child_commits: Vec<ChildCommit> = children_details
            .lines()
            .map(|commit| {
                let details = String::from_utf8(
                    Command::new("git")
                        .args(&["log", "-n", "1", "--pretty=%ci%x09%H%x09%s", &commit])
                        .current_dir(&rls_repo_path)
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
                    .current_dir(&rls_repo_path)
                    .output()
                    .unwrap()
                    .status
                    .success();

                ChildCommit {
                    detailed: CommitWithDetails {
                        commit: commit.to_string(),
                        details: details.trim().to_owned(),
                    },
                    is_ancestor,
                }
            })
            .collect();

        commits.entry(&parent_release).or_default().push((
            CommitWithDetails {
                commit: rust_commit_hash.to_owned(),
                details: parent_details.trim().to_owned(),
            },
            child_commits,
        ));
    }

    println!("{}", Summary(&commits));
    println!(">>> Reduced");
    println!("{}", Summary(&BTreeMap::from_iter(commits.into_iter().filter(|(_, commits)| {
        commits
            .iter()
            .flat_map(|(_, children)| children)
            .any(|child| !child.is_ancestor)
    }))));
}
