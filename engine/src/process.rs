/*!
 * Processing the markdown and resolving references.
 */

use std::{
    collections::HashSet,
    fs::File,
    io::{Read, Write},
    path::{Path, PathBuf},
};

use pulldown_cmark::{html, Event, LinkType, Parser, Tag};
use url::Url;

struct RenderAdapter<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> {
    styles: &'a mut HashSet<&'c str>,
    render_stack: &'a mut Vec<PathBuf>,
    base_dir: &'a Path,
    out_dir: &'a Path,
    filename: &'a Path,
    iter: I,
}

impl<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> Iterator for RenderAdapter<'a, 'b, 'c, I> {
    type Item = Event<'b>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        Some(match item {
            Event::Start(tag) => match tag {
                i @ Tag::Image(..) => {
                    self.styles.insert("image");
                    Event::Start(i)
                }
                p @ Tag::Paragraph => {
                    self.styles.insert("paragraph");
                    Event::Start(p)
                }
                Tag::Link(ty, url, title) => {
                    self.styles.insert("link");
                    match ty {
                        LinkType::Inline => {
                            if let Ok(parsed) = Url::parse(&url) {
                                // check if scheme is hyperref, if so add to stack and rewrite url
                                if parsed.scheme() == "hyperref" {
                                    let parsed_path: &Path = parsed.path().as_ref();
                                    let fname: PathBuf = if parsed_path.is_absolute() {
                                        self.out_dir.join(parsed_path.strip_prefix("/").unwrap())
                                    } else {
                                        self.filename
                                            .parent()
                                            .unwrap_or("/".as_ref())
                                            .join(parsed.path())
                                    }
                                    .with_extension("md");
                                    #[cfg(target_os = "windows")]
                                    // replace with backslashes so that \\?\ isn't broken
                                    let fname: PathBuf =
                                        fname.to_str().unwrap().replace("/", "\\").into();
                                    if let Ok(fname) = fname.canonicalize() {
                                        let fname_for_url =
                                            fname.strip_prefix(&self.base_dir).unwrap();
                                        #[cfg(target_os = "windows")]
                                    // windows is dumb again
                                    let fname_for_url: PathBuf =
                                        fname_for_url.to_str().unwrap().replace("\\", "/").into();
                                        // figure out new location
                                        let new_location = format!(
                                            "/{}",
                                            fname_for_url.with_extension("html").to_str().unwrap(),
                                        );
                                        if !self.render_stack.contains(&fname) {
                                            println!(
                                                "walk: {}",
                                                fname.to_str().unwrap_or("unknown")
                                            );
                                            self.render_stack.push(fname);
                                        }
                                        Event::Start(Tag::Link(ty, new_location.into(), title))
                                        // link.url = new_location.into_bytes();
                                    } else {
                                        println!("Couldn't resolve hyperref: {}", url);
                                        Event::Start(Tag::Link(ty, url, title))
                                    }
                                } else {
                                    Event::Start(Tag::Link(ty, url, title))
                                }
                            } else {
                                Event::Start(Tag::Link(ty, url, title))
                            }
                        }
                        _ => Event::Start(Tag::Link(ty, url, title)),
                    }
                }
                t => Event::Start(t),
            },
            Event::Text(s) => {
                let new_text = s.replace(":eyes:", "👀");
                Event::Text(new_text.into())
            }
            ev => ev,
        })
    }
}

pub fn render(
    filename: &Path,
    base_dir: &Path,
    out_dir: &Path,
    prelude_html: &Path,
    style_chunks_root: &Path,
    base: &str,
    force: bool,
) -> anyhow::Result<()> {
    if !filename.exists() {
        println!(
            "nonexistent: {}, nothing to do",
            filename.to_str().unwrap_or("unknown")
        );
        return Ok(());
    }

    // create out dir if doesn't exist
    if !out_dir.exists() {
        std::fs::create_dir_all(out_dir)?;
    }
    // // canonicalize paths
    // let (filename, base_dir, out_dir) = (
    //     filename.as_ref().canonicalize()?,
    //     base_dir.as_ref().canonicalize()?,
    //     out_dir.as_ref().canonicalize()?,
    // );
    // as-ref paths
    // let (filename, base_dir, out_dir) = (filename.as_ref(), base_dir.as_ref(), out_dir.as_ref());

    // NOTE: can't canonicalize here since the output path may not exist
    let out_path = out_dir
        .join(filename.strip_prefix(&base_dir)?)
        .with_extension("html");
    println!("out: {}", out_path.to_str().unwrap_or("unknown"));

    let buf = {
        let mut s = String::new();
        let mut f = File::open(&filename)?;
        f.read_to_string(&mut s)?;
        Ok::<_, std::io::Error>(s)
    }?;

    let mut styles = {
        let mut h = HashSet::new();
        h.insert("_global");
        h
    };
    let mut stack = Vec::new();

    let html = {
        let parser = Parser::new(&buf);
        let adapter = RenderAdapter {
            base_dir,
            filename,
            out_dir,
            render_stack: &mut stack,
            styles: &mut styles,
            iter: parser,
        };

        let mut s = String::new();
        html::push_html(&mut s, adapter);
        s
    };

    let styles = {
        let mut new_styles = Vec::new();
        for sname in styles.into_iter() {
            let path = style_chunks_root.join(sname).with_extension("css");
            // skip missing files
            if let Ok(path) = AsRef::<Path>::as_ref(&path).canonicalize() {
                let css_out_path = out_dir.join("css").join(sname).with_extension("css");
                if let Some(p) = css_out_path.parent() {
                    std::fs::create_dir_all(p)?;
                }
                std::fs::copy(&path, &css_out_path)?;
                new_styles.push(format!(
                    r#"
<link rel="preload" href="/{0}" as="style" />
<link rel="stylesheet" type="text/css" href="/{0}" />
"#,
                    css_out_path
                        .strip_prefix(out_dir)
                        .unwrap_or(&css_out_path)
                        .to_str()
                        .unwrap_or("unknown")
                        .replace("\\", "/")
                ));
            }
        }
        Ok::<_, std::io::Error>(new_styles)
    }?;
    let html = {
        println!("path = {}", prelude_html.to_str().unwrap_or("unknown"));
        let mut f = File::open(prelude_html)?;
        let mut s = String::new();
        f.read_to_string(&mut s)?;
        Ok::<_, std::io::Error>(s)
    }?
    .replace("@@@SLOT_STYLES@@@", &format!("\n{}\n", styles.join("\n")))
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
        let new_base = format!("{}/{}", base, base_add.to_str().unwrap());
        render(
            &fname,
            base_dir,
            out_dir,
            prelude_html,
            style_chunks_root,
            &new_base,
            force,
        )?;
    }

    Ok(())
}
