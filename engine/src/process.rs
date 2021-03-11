/*!
 * Processing the markdown and resolving references.
 */

use std::{
    collections::{HashSet, VecDeque},
    fs::File,
    io::{Cursor, Read, Write},
    path::{Path, PathBuf},
};

use anyhow::Context;
use image::ImageFormat;
use pulldown_cmark::{html, Event, LinkType, Parser, Tag};
use regex::{Captures, Regex};
use reqwest::blocking::Client;
use tracing::{event, instrument, Level};
use url::Url;

use crate::config::ResolvedConfig;

struct RenderAdapter<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> {
    ctx: &'a mut ProcessorContext<'a, 'c>,
    iter: I,
}

impl<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> Iterator for RenderAdapter<'a, 'b, 'c, I> {
    type Item = Event<'b>;

    #[instrument(name = "process", skip(self))]
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
                Tag::Image(ty, url, title) => {
                    styles.insert("image");
                    match ty {
                        LinkType::Inline => {
                            if let Ok(parsed) = Url::parse(&url) {
                                use sha2::Digest;
                                let hashname = format!(
                                    "{:x}",
                                    sha2::Sha256::digest(parsed.as_str().as_bytes())
                                );
                                let new_url = format!("/images/{}.webp", hashname);
                                let input = RenderingInput::Image {
                                    input: parsed,
                                    output: hashname,
                                };
                                if !render_stack.contains(&input) && !finished.contains(&input) {
                                    render_stack.push_front(input);
                                }
                                Event::Start(Tag::Image(ty, new_url.into(), title))
                            } else {
                                Event::Start(Tag::Image(ty, url, title))
                            }
                        }
                        _ => Event::Start(Tag::Image(ty, url, title)),
                    }
                }
                p @ Tag::Paragraph => {
                    styles.insert("paragraph");
                    Event::Start(p)
                }
                Tag::Heading(level) => {
                    match level {
                        1 => {
                            styles.insert("h1");
                        }
                        _ => {}
                    }
                    Event::Start(Tag::Heading(level))
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
                                        let input = RenderingInput::Page(fname);
                                        if !render_stack.contains(&input)
                                            && !finished.contains(&input)
                                        {
                                            match input {
                                                RenderingInput::Page(ref fname) => {
                                                    event!(Level::INFO, r#type = "walk", ?fname)
                                                }
                                                _ => {}
                                            }
                                            render_stack.push_back(input);
                                        }
                                        Event::Start(Tag::Link(ty, new_location.into(), title))
                                        // link.url = new_location.into_bytes();
                                    } else {
                                        event!(Level::WARN, r#type = "invalid_hyperref", %url);
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
    render_stack: &'a mut VecDeque<RenderingInput>,
}

/// Rendering input
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub enum RenderingInput {
    Index,
    Keep,
    Image {
        input: Url,
        // Will be output to /images/{output}.webp
        output: String,
    },
    Font {
        input: Url,
        // Will be output to /fonts/{output}
        output: String,
    },
    // CSS(chunk_name)
    Style(&'static str),
    Page(PathBuf),
}

/// Processes files
#[derive(Debug)]
pub struct Processor {
    /// Stuff is derived from this
    config: ResolvedConfig,
    // next items to render
    render_stack: VecDeque<RenderingInput>,
    // items that have already been rendered
    finished: HashSet<RenderingInput>,
    // request client
    client: Client,
}

impl Processor {
    pub fn new(config: ResolvedConfig) -> Self {
        Self {
            config,
            render_stack: Default::default(),
            finished: Default::default(),
            client: Client::new(),
        }
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub fn render_toplevel(&mut self, force: bool) -> anyhow::Result<()> {
        self.render_stack.push_front(RenderingInput::Index);
        self.render_stack.push_front(RenderingInput::Keep);
        self.render_all(force)?;
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self))]
    fn render_all(&mut self, force: bool) -> anyhow::Result<()> {
        while let Some(input) = self.render_stack.pop_front() {
            event!(Level::INFO, r#type = "render", ?input);
            self.render(input, force)?;
        }
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self), name = "process_image")]
    fn render_image(&mut self, input: RenderingInput, force: bool) -> anyhow::Result<()> {
        let (inp, out) = match input {
            RenderingInput::Image {
                ref input,
                ref output,
            } => (input, output),
            _ => panic!("expected image enum"),
        };
        let out = PathBuf::from(out).with_extension("webp");
        let out_path = self.config.roots.output.join("images").join(out);

        if out_path.exists() && !force {
            event!(Level::INFO, r#type = "fresh", path = ?out_path);
            self.finished.insert(input);
            return Ok(());
        }

        let (mut reader, img_type): (Box<dyn Read>, ImageFormat) = if inp.scheme() == "file" {
            let path = inp.to_file_path().ok().context("URL to file path")?;
            let f = File::open(&path)?;
            (Box::new(f), ImageFormat::from_path(&path)?)
        } else {
            // fetch the url
            let r = self.client.get(inp.as_str()).send()?;
            let content_type = r
                .headers()
                .get(reqwest::header::CONTENT_TYPE)
                .context("Get image content type")?
                .to_str()?;
            let img_type = match content_type {
                "image/webp" => ImageFormat::WebP,
                "image/png" => ImageFormat::Png,
                "image/jpeg" => ImageFormat::Jpeg,
                "image/gif" => ImageFormat::Gif,
                _ => {
                    return Err(anyhow::anyhow!(
                        "Unknown content type for image: {}",
                        content_type
                    ))
                }
            };
            (Box::new(r), img_type)
        };

        use std::time::Instant;
        let start_time = Instant::now();

        match img_type {
            ImageFormat::WebP => {
                // Directly copy to the file.
                let mut f = File::create(&out_path)?;
                std::io::copy(&mut reader, &mut f)?;
            }
            img_type => {
                // Convert to WebP, then write to file.
                let mut v = Vec::new();
                reader.read_to_end(&mut v)?;
                let cursor = Cursor::new(&v);
                let mut img_in = image::io::Reader::new(cursor);
                img_in.set_format(img_type);
                if let Some(parent) = out_path.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let mut f = File::create(&out_path)?;
                let decoded = img_in.decode()?;
                let encoder = webp::Encoder::from_image(&decoded);
                let mem = encoder.encode(75.);
                f.write_all(&mem)?;
                event!(
                    Level::INFO,
                    r#type = "webp_process",
                    initial_len = v.len(),
                    new_len = mem.len(),
                    change = %((mem.len() as f64) - (v.len() as f64)) / (v.len() as f64) * 100.
                );
            }
        }

        let end_time = Instant::now();
        event!(Level::INFO, r#type = "image_process", path = ?out_path, time = %(end_time - start_time).as_secs_f64());

        self.finished.insert(input);
        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self), name = "process_style")]
    fn render_style(&mut self, input: RenderingInput, force: bool) -> anyhow::Result<()> {
        let sname = match input {
            RenderingInput::Style(sname) => sname,
            _ => panic!("Expected style input"),
        };
        let path = self
            .config
            .lib
            .styles
            .chunks_root
            .join(sname)
            .with_extension("css");
        let out_path = self
            .config
            .roots
            .output
            .join("css")
            .join(sname)
            .with_extension("css");

        if !path.exists() {
            event!(Level::INFO, r#type = "nonexistent_source", ?path);
            self.finished.insert(input);
            return Ok(());
        }

        if !force
            && out_path.exists()
            && out_path.metadata()?.modified()? > path.metadata()?.modified()?
        {
            event!(Level::INFO, r#type = "fresh", path = ?out_path);
            self.finished.insert(input);
            return Ok(());
        }

        // Read file and check for special decls
        let buf = {
            let mut s = String::new();
            let mut f = File::open(&path)?;
            f.read_to_string(&mut s)?;
            s
        };
        let re = Regex::new(r"/\*\*.*@font (?P<url>\S+).*\*/")?;
        let buf = re.replace_all(&buf, |capture: &Captures| {
            let url = capture.name("url").unwrap();
            // Fetch URL
            let contents = {
                let mut s = String::new();
                let mut r = self.client.get(url.as_str()).send().unwrap();
                r.read_to_string(&mut s).unwrap();
                s
            };
            // Match font URLs inside...
            let re2 = Regex::new(r"url\((?P<url>\S+)\)").unwrap();
            let contents = re2.replace_all(&contents, |captures: &Captures| {
                let input = captures.name("url").unwrap();
                use sha2::Digest;
                let hashname = format!("{:x}", sha2::Sha256::digest(input.as_str().as_bytes()));
                let parsed = Url::parse(input.as_str()).unwrap();
                let output_filename = format!(
                    "{}.{}",
                    hashname,
                    parsed
                        .path_segments()
                        .unwrap()
                        .last()
                        .unwrap()
                        .split(".")
                        .last()
                        .unwrap()
                );
                let input = RenderingInput::Font {
                    input: parsed,
                    output: output_filename,
                };
                let output_filename = match input {
                    RenderingInput::Font { ref output, .. } => output,
                    _ => unreachable!(),
                };
                let new_url = format!("url(/fonts/{})", output_filename);
                if !self.render_stack.contains(&input) && !self.finished.contains(&input) {
                    self.render_stack.push_front(input);
                }
                new_url
            });
            contents.to_string()
        });

        if let Some(p) = out_path.parent() {
            std::fs::create_dir_all(p)?;
        }
        // Minify style first
        let minified_css = {
            let minified =
                html_minifier::css::minify(&buf).map_err(|_| anyhow::anyhow!("minify failed"))?;
            event!(
                Level::INFO,
                r#type = "minified",
                in_len = buf.len(),
                new_len = minified.len(),
                change = %(((minified.len() as f64) - (buf.len() as f64)) / buf.len() as f64) * 100.
            );
            Ok::<_, anyhow::Error>(minified)
        }?;
        let mut f = File::create(&out_path)?;
        f.write_all(minified_css.as_bytes())?;

        self.finished.insert(input);
        event!(Level::INFO, r#type = "new", path = ?out_path);

        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self), name = "process_font")]
    fn render_font(&mut self, input: RenderingInput, force: bool) -> anyhow::Result<()> {
        // Just download the file to the given path
        let (url, output) = match input {
            RenderingInput::Font {
                ref input,
                ref output,
            } => (input, output),
            _ => panic!("Expected font"),
        };
        let out_path = self.config.roots.output.join("fonts").join(output);

        if !force && out_path.exists() {
            event!(Level::INFO, r#type = "fresh", %url);
            self.finished.insert(input);
            return Ok(());
        }

        let mut r = self.client.get(url.as_str()).send()?;
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut f = File::create(&out_path)?;
        std::io::copy(&mut r, &mut f)?;

        self.finished.insert(input);
        event!(Level::INFO, r#type = "new", path = ?out_path);

        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self))]
    fn render(&mut self, input: RenderingInput, force: bool) -> anyhow::Result<()> {
        let out_dir = &self.config.roots.output;
        let base_dir = &self.config.roots.source;
        let style_chunks_root = &self.config.lib.styles.chunks_root;
        let prelude_html = &self.config.lib.prelude_location;
        let filename = match input {
            RenderingInput::Index => &self.config.inputs.index,
            RenderingInput::Keep => &self.config.inputs.keep,
            RenderingInput::Style(..) => return self.render_style(input, force),
            RenderingInput::Font { .. } => return self.render_font(input, force),
            RenderingInput::Image { .. } => return self.render_image(input, force),
            RenderingInput::Page(ref o) => o,
        };

        if !filename.exists() {
            event!(Level::INFO, r#type = "nonexistent_source", path = ?filename);
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
                if let Ok(_) = AsRef::<Path>::as_ref(&path).canonicalize() {
                    let css_out_path = out_dir.join("css").join(sname).with_extension("css");
                    let input = RenderingInput::Style(sname);
                    if !self.render_stack.contains(&input) && !self.finished.contains(&input) {
                        self.render_stack.push_front(input);
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
            let mut f = File::open(prelude_html)?;
            let mut s = String::new();
            f.read_to_string(&mut s)?;
            Ok::<_, std::io::Error>(s)
        }?
        .replace("@@@SLOT_STYLES@@@", &format!("\n{}\n", styles.join("\n")))
        .replace("@@@SLOT_CONTENT@@@", &html);

        // Minify HTML
        let minified = html_minifier::minify(&html)?;

        event!(
            Level::INFO,
            r#type = "minified",
            in_len = html.len(),
            new_len = minified.len(),
            change = %(((minified.len() as f64) - (html.len() as f64)) / html.len() as f64) * 100.
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
            event!(Level::INFO, r#type = "fresh", path = ?out_path);
        } else {
            // first, recursively create parents
            if let Some(p) = out_path.parent() {
                std::fs::create_dir_all(p)?;
            }

            if input == RenderingInput::Keep {
                event!(Level::INFO, r#type = "special_keep", path = ?out_path);
            } else {
                let mut f = File::create(&out_path)?;
                f.write_all(minified.as_bytes())?;
                // println!("{}", html);
                event!(Level::INFO, r#type = "new", path = ?out_path);
            }
        }

        self.finished.insert(input);

        Ok(())
    }
}
