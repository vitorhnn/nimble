# Nimble

Nimble is a Swifty-compatible, cross platform, (currently) CLI only mod manager for Arma 3.

# Installing

Binaries are available at the [Release](https://github.com/vitorhnn/nimble/releases) page in GitHub.

Nimble can also be installed via Cargo:

```
cargo install --git https://github.com/vitorhnn/nimble.git
```

# Usage

## Mod synchronization

Unlike Swifty, Nimble (currently) is not capable of detecting when a repo is outdated,
so whenever your group pushes updates or when first installing, you must run:

```
nimble sync --repo-url <your group's repository URL> --path <path to where mods will be stored>
```

### Storage path restriction
For Linux under Proton, the mod storage path must be inside Arma 3's Proton prefix "drive_c", e.g:
```
nimble sync --repo-url https://example.com/swifty/ --path /home/foo/.local/share/Steam/steamapps/compatdata/107410/pfx/drive_c/arma_mods
```

This restriction will be removed in the future.

## Arma 3 launching

On Windows and Linux with Proton, Nimble can launch Arma 3 using the `steam://` protocol:

```
nimble launch --path <mod storage path>
```

## SRF generation

The mod cache can be forcefully regenerated if required:
```
nimble gen-srf --path <mod storage path>
```

This should only be needed if you manually made changes to the mods.
