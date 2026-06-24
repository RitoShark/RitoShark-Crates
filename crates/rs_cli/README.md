# rs_cli

The RitoShark command-line tool. A thin binary over the RitoShark crates that detects, reads,
and converts League of Legends file formats entirely in-process — it never invokes an external
program.

## Build & run

```
cargo run -p rs_cli -- read <file>
cargo build -p rs_cli --release   # target/release/rs_cli[.exe]
```

## Quick reference

```
rs_cli read <file> [--json] [--hashes <dir>]
rs_cli detect <file> [--json]
rs_cli transform <in> [out] [-r] [--keep-hashed]

rs_cli bin convert <in> [out] [-r] [--keep-hashed]
rs_cli bin diff <a> <b> [-C <n>] [--no-color]
rs_cli wad list <archive>... [-F table|json|csv|flat] [--stats]
rs_cli wad extract <archive>... -o <dir> [-f <type>...] [-x <regex>] [--overwrite]
rs_cli tex info <in> [--json]
rs_cli tex decode <in> [-o <out>] [--mip <n>]
rs_cli tex encode <in> -f <bc1|bc3|bc5|bc7|bgra8> [-m] [-o <out>]
rs_cli rst list <in> [--json]
rs_cli audio extract <wpk|bnk> -o <dir>
```

See [`../../docs/cli.md`](../../docs/cli.md) for the full command reference.
