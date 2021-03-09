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

use crate::config::ResolvedConfig;

struct RenderAdapter<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> {
    ctx: &'a mut ProcessorContext<'a, 'c>,
    iter: I,
}

impl<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> Iterator for RenderAdapter<'a, 'b, 'c, I> {
    type Item = Event<'b>;

    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        let styles = &mut self.ctx.styles;
        let render_stack = &mut *self.ctx.render_stack;
        let finished = self.ctx.finished;
        let out_dir = &self.ctx.config.roots.output;
        let base_dir = &self.ctx.config.roots.source;
        let filename = self.ctx.filename;
        Some(match item {
            Event::Start(tag) => match tag {
                i @ Tag::Image(..) => {
                    styles.insert("image");
                    Event::Start(i)
                }
                p @ Tag::Paragraph => {
                    styles.insert("paragraph");
                    Event::Start(p)
                }
                Tag::Link(ty, url, title) => {
                    styles.insert("link");
                    match ty {
                        LinkType::Inline => {
                            if let Ok(parsed) = Url::parse(&url) {
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
                                    let fname: PathBuf =
                                        fname.to_str().unwrap().replace("/", "\\").into();
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
                                        let input = RenderingInput::Other(fname);
                                        if !render_stack.contains(&input)
                                            && !finished.contains(&input)
                                        {
                                            match input {
                                                RenderingInput::Other(ref fname) => println!(
                                                    "walk: {}",
                                                    fname.to_str().unwrap_or("unknown")
                                                ),
                                                _ => {}
                                            }
                                            render_stack.push(input);
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
            ev => ev,
        })
    }
}

/// Processing context for a single file
pub struct ProcessorContext<'a, 'b: 'a> {
    styles: &'a mut HashSet<&'b str>,
    filename: &'a Path,
    config: &'a ResolvedConfig,
    finished: &'a HashSet<RenderingInput>,
    render_stack: &'a mut Vec<RenderingInput>,
}

/// Rendering input
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum RenderingInput {
    Index,
    Keep,
    Other(PathBuf),
}

/// Processes files
pub struct Processor {
    /// Stuff is derived from this
    config: ResolvedConfig,
    // next items to render
    render_stack: Vec<RenderingInput>,
    // items that have already been rendered
    finished: HashSet<RenderingInput>,
}

impl Processor {
    pub fn new(config: ResolvedConfig) -> Self {
        Self {
            config,
            render_stack: Default::default(),
            finished: Default::default(),
        }
    }

    pub fn render_toplevel(&mut self, force: bool) -> anyhow::Result<()> {
        self.render_stack.extend(std::array::IntoIter::new([
            RenderingInput::Index,
            RenderingInput::Keep,
        ]));
        self.render_all(force)?;
        Ok(())
    }

    fn render_all(&mut self, force: bool) -> anyhow::Result<()> {
        while !self.render_stack.is_empty() {
            let stack = std::mem::take(&mut self.render_stack);
            for input in stack {
                println!("RENDER: {:?}", input);
                self.render(input, force)?;
            }
        }
        Ok(())
    }

    pub fn render(&mut self, input: RenderingInput, force: bool) -> anyhow::Result<()> {
        let out_dir = &self.config.roots.output;
        let base_dir = &self.config.roots.source;
        let style_chunks_root = &self.config.lib.styles.chunks_root;
        let prelude_html = &self.config.lib.prelude_location;
        let filename = match input {
            RenderingInput::Index => &self.config.inputs.index,
            RenderingInput::Keep => &self.config.inputs.keep,
            RenderingInput::Other(ref o) => o,
        };

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

        let html = {
            let parser = Parser::new(&buf);
            let mut ctx = ProcessorContext {
                filename,
                styles: &mut styles,
                config: &self.config,
                finished: &self.finished,
                render_stack: &mut self.render_stack,
            };
            let adapter = RenderAdapter {
                ctx: &mut ctx,
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
                    // Minify style first
                    let minified_css = {
                        let mut s = String::new();
                        let mut f = File::open(&path)?;
                        f.read_to_string(&mut s)?;
                        let minified = html_minifier::css::minify(&s)
                            .map_err(|_| anyhow::anyhow!("minify failed"))?;
                        println!(
                            "Minified {} bytes -> {} bytes ({:.3}%)",
                            s.len(),
                            minified.len(),
                            (((minified.len() as f64) - (s.len() as f64)) / s.len() as f64) * 100.
                        );
                        Ok::<_, anyhow::Error>(minified)
                    }?;
                    {
                        let mut f = File::create(&css_out_path)?;
                        f.write_all(minified_css.as_bytes())?;
                    }
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

        // Minify HTML
        let minified = html_minifier::minify(&html)?;

        println!(
            "Minified {} bytes -> {} bytes ({:.3}%)",
            html.len(),
            minified.len(),
            (((minified.len() as f64) - (html.len() as f64)) / html.len() as f64) * 100.
        );

        // write only if file doesn't exist
        let needs_update = if let (Ok(out_metadata), Ok(in_metadata)) =
            (out_path.metadata(), filename.metadata())
        {
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
                f.write_all(minified.as_bytes())?;
                // println!("{}", html);
                println!("new: {}", out_path.to_str().unwrap_or("unknown"));
            }
        }

        self.finished.insert(input);

        Ok(())
    }
}
