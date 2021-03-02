use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use clap::Clap;
use comrak::{
    nodes::{AstNode, NodeValue},
    Arena, ComrakOptions,
};
use url::Url;

#[derive(Clap)]
#[clap(
    version = "0.1",
    author = "Anirudh Balaji <anirudhb@users.noreply.github.com>"
)]
struct Args {
    input_filename: PathBuf,
    #[clap(short, long)]
    /// Forces rebuild
    force: bool,
}

fn walk<'a>(node: &'a AstNode<'a>, f: &mut impl FnMut(&'a AstNode<'a>)) {
    f(node);
    for c in node.children() {
        walk(c, f);
    }
}

fn main() -> anyhow::Result<()> {
    let args = Args::parse();
    println!("Input filename: {}", args.input_filename.to_string_lossy());
    render(
        &args.input_filename,
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/../out"),
        "",
        args.force,
    )?;
    render(
        args.input_filename.with_file_name("_keep.md"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/../src"),
        concat!(env!("CARGO_MANIFEST_DIR"), "/../out"),
        "",
        args.force,
    )?;

    Ok(())
}

fn render<P1: AsRef<Path>, P2: AsRef<Path>, P3: AsRef<Path>, S: AsRef<str>>(
    filename: P1,
    base_dir: P2,
    out_dir: P3,
    base: S,
    force: bool,
) -> anyhow::Result<()> {
    if !filename.as_ref().exists() {
        println!(
            "nonexistent: {}, nothing to do",
            filename.as_ref().to_string_lossy()
        );
        return Ok(());
    }

    // create out dir if doesn't exist
    if !out_dir.as_ref().exists() {
        std::fs::create_dir_all(out_dir.as_ref())?;
    }
    // canonicalize paths
    let (filename, base_dir, out_dir) = (
        filename.as_ref().canonicalize()?,
        base_dir.as_ref().canonicalize()?,
        out_dir.as_ref().canonicalize()?,
    );

    let out_path = out_dir
        .join(filename.strip_prefix(&base_dir)?)
        .with_extension("html");
    println!("out: {}", out_path.to_string_lossy());

    let buf = {
        let mut s = String::new();
        let mut f = File::open(&filename)?;
        f.read_to_string(&mut s)?;
        Ok::<_, std::io::Error>(s)
    }?;

    let arena = Arena::new();
    let root = comrak::parse_document(&arena, &buf, &ComrakOptions::default());

    let mut styles = {
        let mut h = HashSet::new();
        h.insert("_global");
        h
    };
    let mut stack = Vec::new();

    walk(root, &mut |node| {
        match node.data.borrow_mut().value {
            NodeValue::Image(..) => {
                styles.insert("image");
            }
            NodeValue::Link(ref mut link) => {
                styles.insert("link");
                // check link
                // okay since entire document is a String
                let url_s = std::str::from_utf8(&link.url).unwrap();
                if let Ok(parsed) = Url::parse(url_s) {
                    // check if scheme is hyperref, if so add to stack and rewrite url
                    if parsed.scheme() == "hyperref" {
                        let parsed_path: &Path = parsed.path().as_ref();
                        let fname: PathBuf = if parsed_path.is_absolute() {
                            out_dir.join(parsed_path.strip_prefix("/").unwrap())
                        } else {
                            filename
                                .parent()
                                .unwrap_or("/".as_ref())
                                .join(parsed.path())
                        }
                        .with_extension("md");
                        #[cfg(target_os = "windows")]
                        // replace with backslashes so that \\?\ isn't broken
                        let fname: PathBuf = fname.to_str().unwrap().replace("/", "\\").into();
                        if let Ok(fname) = fname.canonicalize() {
                            let fname_for_url = fname.strip_prefix(&base_dir).unwrap();
                            #[cfg(target_os = "windows")]
                            // windows is dumb again
                            let fname_for_url: PathBuf =
                                fname_for_url.to_str().unwrap().replace("\\", "/").into();
                            // figure out new location
                            let new_location = format!(
                                "/{}",
                                fname_for_url.with_extension("html").to_str().unwrap(),
                            );
                            if !stack.contains(&fname) {
                                println!("walk: {}", fname.to_string_lossy());
                                stack.push(fname);
                            }
                            link.url = new_location.into_bytes();
                        } else {
                            println!("Couldn't resolve hyperref: {}", url_s);
                        }
                    }
                }
            }
            NodeValue::Paragraph => {
                styles.insert("paragraph");
            }
            NodeValue::Text(ref mut text) => {
                // okay since the entire document is a String
                let s = std::str::from_utf8(&text).unwrap();
                let new_text = s.replace(":eyes:", "👀");
                *text = new_text.into_bytes();
            }
            _ => {}
        };
    });

    let html = {
        let mut html = Vec::new();
        comrak::format_html(root, &ComrakOptions::default(), &mut html)?;
        let s = String::from_utf8(html)?;
        Ok::<_, anyhow::Error>(s)
    }?;
    let styles = {
        let mut new_styles = Vec::new();
        for sname in styles.into_iter() {
            let mut s = String::new();
            let path = format!(
                "{}/../lib/style-chunks/{}.css",
                env!("CARGO_MANIFEST_DIR"),
                sname
            );
            if !AsRef::<Path>::as_ref(&path).exists() {
                // skip missing files!
                continue;
            }
            let mut f = File::open(&path)?;
            f.read_to_string(&mut s)?;
            new_styles.push(s);
        }
        Ok::<_, std::io::Error>(new_styles)
    }?;
    let html = {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../lib/prelude.html");
        println!("path = {}", path);
        let mut f = File::open(path)?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        Ok::<_, std::io::Error>(s)
    }?
    .replace(
        "@@@SLOT_STYLES@@@",
        &format!("<style>\n{}\n</style>", styles.join("\n")),
    )
    .replace("@@@SLOT_CONTENT@@@", &html);

    // write only if file doesn't exist
    let needs_update =
        if let (Ok(out_metadata), Ok(in_metadata)) = (out_path.metadata(), filename.metadata()) {
            in_metadata.modified()? >= out_metadata.modified()?
        } else {
            // failed to get metadata, or either path doesn't exist
            true
        };
    if !needs_update && !force {
        // nothing to do
        println!("fresh: {}", out_path.to_str().unwrap_or("unknown"));
    } else {
        // first, recursively create parents
        if let Some(p) = out_path.parent() {
            std::fs::create_dir_all(p)?;
        }

        if out_path.file_stem().unwrap().to_str().unwrap() == "_keep" {
            println!("special: _keep, no output");
        } else {
            let mut f = File::create(&out_path)?;
            f.write_all(html.as_bytes())?;
            // println!("{}", html);
            println!("new: {}", out_path.to_str().unwrap_or("unknown"));
        }
    }

    // write stack
    for fname in stack {
        let base_add = fname.strip_prefix(&base_dir).unwrap();
        let new_base = format!("{}/{}", base.as_ref(), base_add.to_str().unwrap());
        render(fname, &base_dir, &out_dir, new_base, force)?;
    }

    Ok(())
}
