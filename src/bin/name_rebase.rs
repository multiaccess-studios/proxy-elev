use std::path::PathBuf;

use anyhow::Context;
use clap::Parser;

#[derive(Parser, Debug)]
struct Opt {
    files: PathBuf,
    ext: String,
    offset: i32,
}

fn main() -> anyhow::Result<()> {
    let opt = Opt::parse();

    let files = std::fs::read_dir(&opt.files)?;

    let ext = format!(".{}", opt.ext);

    for file in files {
        let file = file?.path();
        let Some(name) = file.file_name() else {
            continue;
        };
        let name = name.to_str().context("`file_name` not a string")?;
        let Some(name) = name.strip_suffix(&ext) else {
            continue;
        };

        let out_name = if let Some((num, rest)) = name.split_once('.') {
            let num = num
                .parse::<i32>()
                .context(format!("`{num}` not a number"))?;
            let num = num + opt.offset;
            format!("{num}.{rest}{ext}")
        } else {
            let num = name
                .parse::<i32>()
                .context(format!("`{name}` not a number"))?;
            let num = num + opt.offset;
            format!("{num}{ext}")
        };
        std::fs::rename(&file, file.with_file_name(out_name))?;
    }

    Ok(())
}
