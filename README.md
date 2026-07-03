# replicube-tokenize

A CLI and Rust crate for tokenizing the game Replicube's Lua code, with cost reporting

## Installation

```sh
cargo install --git https://github.com/akouryy/replicube-tokenize
```

## CLI

```console
$ replicube-tokenize 'return 0x10.10p3 + 1' -f short
6
$ replicube-tokenize 'return 0x10.10p3 + 1' -f long
1   return
3   0x10.10p3
1   +
1   1
---
6
```

Warnings go to stderr.

## Library

```rust
let (tokens, warnings) = replicube_tokenize::tokenize("return ({1,2})[x]");
let cost: Option<usize> = tokens.iter().map(|t| t.cost()).sum();
```
