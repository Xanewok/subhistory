
```
$ git --git-dir=/home/xanewok/repos/rust/.git log --pretty=%H -p --submodule -- src/tools/rls | rg '^\w' | cargo run --release
```

