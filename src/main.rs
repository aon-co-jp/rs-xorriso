mod iso9660;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

struct Args {
    source_dir: PathBuf,
    output: PathBuf,
    volume_label: String,
}

fn parse_args() -> Result<Args, String> {
    let raw: Vec<String> = env::args().skip(1).collect();

    // xorrisoに親しんだ利用者のための簡易互換: `-as mkisofs -V LABEL -o OUT SRC`
    let mut output = None;
    let mut volume_label = "RS_XORRISO".to_string();
    let mut source_dir = None;

    let mut i = 0;
    while i < raw.len() {
        match raw[i].as_str() {
            "-as" => { i += 2; } // "-as mkisofs" は互換のため読み飛ばす
            "-o" => { i += 1; output = raw.get(i).map(PathBuf::from); }
            "-V" => { i += 1; volume_label = raw.get(i).cloned().unwrap_or(volume_label); }
            other => { source_dir = Some(PathBuf::from(other)); }
        }
        i += 1;
    }

    Ok(Args {
        source_dir: source_dir.ok_or("ソースディレクトリを指定してください / source directory is required")?,
        output: output.ok_or("出力先(-o)を指定してください / -o <output.iso> is required")?,
        volume_label,
    })
}

fn main() -> ExitCode {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("エラー: {e}");
            eprintln!("使い方 / usage: rs-xorriso -as mkisofs -V LABEL -o output.iso <source_dir>");
            return ExitCode::FAILURE;
        }
    };

    match iso9660::write_iso(&args.source_dir, &args.output, &args.volume_label) {
        Ok(()) => {
            println!("ISOイメージを作成しました / created ISO image: {}", args.output.display());
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("エラー: {e}");
            ExitCode::FAILURE
        }
    }
}
