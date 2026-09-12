mod iso9660;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;

struct Args {
    source_dir: PathBuf,
    output: PathBuf,
    volume_label: String,
}

/// 値を伴わないフラグで、rs-xorrisoでは効果を持たないが互換のため
/// 認識だけはして無視するもの。
/// 重要: `-J`/`-R`(Joliet/Rock Ridge)は本家では長いファイル名を保持するが、
/// rs-xorrisoは常に8.3形式へ切り詰める(Level 1のみ対応、実機検証済みの
/// 既知の非互換)。黙って無視するのではなく警告を出す。
const IGNORED_NOOP_FLAGS: &[&str] = &["-J", "-R", "-joliet", "-rock", "-v", "-q"];
/// 値を1つ伴うフラグで、rs-xorrisoでは効果を持たないが互換のため
/// 認識だけはして無視するもの。
const IGNORED_VALUE_FLAGS: &[&str] = &["-iso-level"];

fn parse_args() -> Result<Args, String> {
    let raw: Vec<String> = env::args().skip(1).collect();

    if raw.first().map(String::as_str) == Some("-as") && raw.get(1).map(String::as_str) == Some("cdrecord") {
        return Err(
            "rs-xorrisoはISOイメージの生成のみに対応しており、ディスクへの実書き込み(-as cdrecord)は未実装です。本家xorrisoを使用してください。 / \
             rs-xorriso only creates ISO images; actual disc burning (-as cdrecord) is not implemented. Please use the real xorriso for that.".to_string()
        );
    }
    if raw.first().map(String::as_str) == Some("-devices") {
        return Err(
            "rs-xorrisoはドライブ列挙(-devices)に未対応です。本家xorrisoを使用してください。 / \
             rs-xorriso does not implement drive enumeration (-devices). Please use the real xorriso for that.".to_string()
        );
    }

    // xorrisoに親しんだ利用者のための簡易互換: `-as mkisofs -V LABEL -o OUT SRC`
    let mut output = None;
    let mut volume_label = "RS_XORRISO".to_string();
    let mut source_dir = None;

    let mut i = 0;
    while i < raw.len() {
        let tok = raw[i].as_str();
        match tok {
            "-as" => { i += 2; } // "-as mkisofs" は互換のため読み飛ばす
            "-o" => { i += 1; output = raw.get(i).map(PathBuf::from); }
            "-V" => { i += 1; volume_label = raw.get(i).cloned().unwrap_or(volume_label); }
            _ if IGNORED_NOOP_FLAGS.contains(&tok) => {
                if tok == "-J" || tok == "-R" || tok == "-joliet" || tok == "-rock" {
                    eprintln!(
                        "warning: '{tok}' はrs-xorrisoでは効果がありません。長いファイル名は8.3形式へ切り詰められます。 / \
                         warning: '{tok}' has no effect in rs-xorriso — long filenames are truncated to 8.3."
                    );
                }
            }
            _ if IGNORED_VALUE_FLAGS.contains(&tok) => { i += 1; }
            other if other.starts_with('-') => {
                return Err(format!(
                    "未対応のオプション '{other}' です / unsupported option '{other}'"
                ));
            }
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
