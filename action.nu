export def check [] {
    cargo build
    cargo test --profile dev
}

export def build [
    --version: string
    ...target: string 
] {
    for t in $target {
        rustup target add $t
        cargo build --release --target $t
        match $t {
            "aarch64-apple-darwin" | "x86_64-apple-darwin" | "x86_64-unknown-linux-gnu" => {
                (tar 
                    -C $"target/($t)/release" 
                    -czvf $"target/precord-($t)-($version).tar.gz" 
                    precord)
            }
            "aarch64-pc-windows-msvc" | "x86_64-pc-windows-msvc" => {
                mv $"target/($t)/release/precord.exe" $"target/precord-($t)-($version).exe"
            }
        }
    }
}
