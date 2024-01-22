# Nimble

Nimble is (or will be) a Swifty-compatible, cross platform, (currently) CLI only mod manager for Arma 3.
It is being built because I want to play with my Swifty using group on Linux :)

## Goals
In order of priority:
 * Full compatibility with Swifty
 * Support all major platforms where you can reasonably run Arma 3 (Windows, Linux w/ Proton, maybe macOS?)
 * Have decent usability
   * This implies some form of user interface system. Rust's GUI story is kinda wonky, so maybe we'll have to settle for a TUI
 * Generate Swifty repositories (swifty-cli create equivalent)
   * Create symlinks instead of copying mod files 

## Big bucket list of things to do:
 * Implement part level downloading, instead of redownloading entire files
 * Clean up Path vs PathBuf vs &str vs String
   * RelativePath and RelativePathBuf should be used in most cases.
   * We still need to convert Windows backslashes to a sane separator on *nix platforms
 * Use rayon for srf generation
   * Somewhat done but needs improvement
 * Properly deal with invalid PBOs