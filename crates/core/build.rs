fn main() {
    // `libgit2-sys` 0.17.0 (pulled in by `git2`) compiles libgit2 from source on
    // Windows and its own build script only emits link directives for
    // winhttp/rpcrt4/ole32/crypt32/secur32. libgit2's C sources additionally
    // reference CryptoAPI, registry and token/SID functions
    // (OpenProcessToken, CheckTokenMembership, CopySid, EqualSid, GetLengthSid,
    // GetTokenInformation, IsValidSid, IsWellKnownSid, GetNamedSecurityInfoW,
    // RegCloseKey, RegOpenKeyExW, RegQueryValueExW, CryptAcquireContextA,
    // CryptReleaseContext, CryptGenRandom) which all live in advapi32.
    // Without an explicit link the final binary fails with LNK2019/LNK1120.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        println!("cargo:rustc-link-lib=advapi32");
    }
}
