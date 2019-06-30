
```
$ git --git-dir=$RUST_REPO_DIR/.git log --pretty=%H -p --submodule -- src/tools/rls | rg '^\w' | cargo run --release
```

