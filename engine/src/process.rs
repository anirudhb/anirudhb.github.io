/*!
 * Processing the markdown and resolving references.
 */

use std::{
    collections::HashSet,
    io::Cursor,
    path::{Path, PathBuf},
    pin::Pin,
    sync::Arc,
};

use anyhow::Context;
use dashmap::DashSet;
use image::ImageFormat;
use pulldown_cmark::{html, Options, Parser};
use regex::{Captures, Regex, RegexBuilder};
use surf::Client;
use syntect::{highlighting::ThemeSet, parsing::SyntaxSet};
use tokio::{
    fs::File,
    io::{AsyncRead, AsyncReadExt, AsyncWriteExt},
    sync::mpsc::UnboundedSender,
};
use tokio_util::compat::FuturesAsyncReadCompatExt;
use tracing::{event, instrument, Level};
use url::Url;

use crate::config::ResolvedConfig;
use crate::frontmatter::DATE_FORMAT;
use crate::render_adapter::{ProcessorContext, RenderAdapter};

/// Rendering input
#[derive(Clone, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
pub(crate) enum RenderingInput {
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
    // items that are currently being rendered
    render_stack: DashSet<RenderingInput>,
    // items that have already been rendered
    finished: DashSet<RenderingInput>,
    // request client
    client: Client,
    // syntax set
    ss: SyntaxSet,
    // theme set
    ts: ThemeSet,
}

const THEMES: &'static [u8] = include_bytes!(concat!(env!("OUT_DIR"), "/themes.themedump"));

impl Processor {
    pub fn new(config: ResolvedConfig) -> anyhow::Result<Arc<Self>> {
        let mut ts = syntect::dumps::from_binary::<ThemeSet>(THEMES);
        if let Some(ref loc) = config.lib.themes_location {
            ts.add_from_folder(loc)?;
        }
        Ok(Arc::new(Self {
            config,
            render_stack: Default::default(),
            finished: Default::default(),
            client: Client::new(),
            ss: SyntaxSet::load_defaults_newlines(),
            ts,
        }))
    }

    #[instrument(level = Level::INFO, skip(self))]
    pub async fn render_toplevel(self: Arc<Self>, force: bool) -> anyhow::Result<()> {
        self.render_stack.insert(RenderingInput::Index);
        self.render_stack.insert(RenderingInput::Keep);
        self.render_all(force).await?;
        Ok(())
    }

    fn spawn_input(
        self: Arc<Self>,
        force: bool,
        input: RenderingInput,
        tx: UnboundedSender<anyhow::Result<()>>,
    ) {
        tokio::spawn(async move {
            let i2 = input.clone();
            let r = self.clone().render(input, force, tx.clone()).await;
            self.render_stack.remove(&i2);
            self.finished.insert(i2);
            tx.send(r).unwrap();
        });
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn render_all(self: Arc<Self>, force: bool) -> anyhow::Result<()> {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
        let stack = {
            let copy = self.render_stack.clone();
            self.render_stack.clear();
            copy
        };
        for input in stack {
            let tx = tx.clone();
            let this = self.clone();
            this.spawn_input(force, input, tx);
        }

        drop(tx);

        while let Some(res) = rx.recv().await {
            res?;
        }

        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self), name = "process_image")]
    async fn render_image(
        self: Arc<Self>,
        input: RenderingInput,
        force: bool,
    ) -> anyhow::Result<()> {
        let (inp, out) = match input {
            RenderingInput::Image {
                ref input,
                ref output,
            } => (input, output),
            _ => panic!("expected image enum"),
        };
        let out = PathBuf::from(out).with_extension("webp");
        let out_path = self.config.roots.output.join("images").join(out);

        if !force && tokio::fs::metadata(&out_path).await.is_ok() {
            event!(Level::INFO, r#type = "fresh", path = ?out_path);
            return Ok(());
        }

        let (mut reader, img_type): (Pin<Box<dyn AsyncRead + Send + Sync>>, ImageFormat) =
            if inp.scheme() == "file" {
                let path = inp.to_file_path().ok().context("URL to file path")?;
                let f = File::open(&path).await?;
                (Box::pin(f), ImageFormat::from_path(&path)?)
            } else {
                // fetch the url
                let r = self
                    .client
                    .get(inp.as_str())
                    .send()
                    .await
                    .map_err(|_| anyhow::anyhow!("fetch failed"))?;
                let content_type = &r.header("Content-Type").context("Get image content type")?[0];
                let img_type = match content_type.as_str() {
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
                (Box::pin(r.compat()), img_type)
            };

        use std::time::Instant;
        let start_time = Instant::now();

        match img_type {
            ImageFormat::WebP => {
                // Directly copy to the file.
                let mut f = File::create(&out_path).await?;
                tokio::io::copy(&mut reader, &mut f).await?;
            }
            img_type => {
                // Convert to WebP, then write to file.
                let mut v = Vec::new();
                reader.read_to_end(&mut v).await?;
                let cursor = Cursor::new(&v);
                let mut img_in = image::io::Reader::new(cursor);
                img_in.set_format(img_type);
                if let Some(parent) = out_path.parent() {
                    tokio::fs::create_dir_all(parent).await?;
                }
                let mut f = File::create(&out_path).await?;
                let decoded = img_in.decode()?;
                // WebP encoding has to be done on a separate thread since it is !Send
                let (tx2, mut rx2) = tokio::sync::mpsc::unbounded_channel();
                std::thread::spawn(move || {
                    let encoder = webp::Encoder::from_image(&decoded);
                    let mem = encoder.encode(75.);
                    tx2.send(mem.to_vec()).unwrap();
                });
                let res = rx2.recv().await.unwrap();
                f.write_all(&res).await?;
                event!(
                    Level::INFO,
                    r#type = "webp_process",
                    initial_len = v.len(),
                    new_len = res.len(),
                    change = %((res.len() as f64) - (v.len() as f64)) / (v.len() as f64) * 100.
                );
            }
        }

        let end_time = Instant::now();
        event!(Level::INFO, r#type = "image_process", path = ?out_path, time = %(end_time - start_time).as_secs_f64());

        Ok(())
    }

    async fn _style_regex_replacer(
        self: Arc<Self>,
        capture: &Captures<'_>,
        force: bool,
        tx: UnboundedSender<anyhow::Result<()>>,
    ) -> anyhow::Result<String> {
        let url = capture.name("url").unwrap();
        // Fetch URL
        let contents = {
            let mut s = String::new();
            let mut r = self
                .client
                .get(url.as_str())
                .send()
                .await
                .map_err(|_| anyhow::anyhow!("fetch failed"))?
                .compat();
            r.read_to_string(&mut s).await?;
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
                self.render_stack.insert(input.clone());
                self.clone().spawn_input(force, input, tx.clone());
            }
            new_url
        });
        Ok(contents.to_string())
    }

    #[instrument(level = Level::INFO, skip(self), name = "process_style")]
    async fn render_style(
        self: Arc<Self>,
        input: RenderingInput,
        force: bool,
        tx: UnboundedSender<anyhow::Result<()>>,
    ) -> anyhow::Result<()> {
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
            return Ok(());
        }

        let out_path_metadata = tokio::fs::metadata(&out_path).await;
        if !force
            && out_path_metadata.is_ok()
            && out_path_metadata?.modified()? > tokio::fs::metadata(&path).await?.modified()?
        {
            event!(Level::INFO, r#type = "fresh", path = ?out_path);
            return Ok(());
        }

        // Read file and check for special decls
        let buf = {
            let mut s = String::new();
            let mut f = File::open(&path).await?;
            f.read_to_string(&mut s).await?;
            s
        };
        let re = Regex::new(r"/\*\*.*@font (?P<url>\S+).*\*/")?;

        // src/regex/re_unicode.rs:569-588, regex crate
        // The slower path, which we use if the replacement needs access to
        // capture groups.
        let buf = {
            use std::borrow::Cow;
            let text = &buf;
            let limit = 0;
            let it = re.captures_iter(text).enumerate().collect::<Vec<_>>();
            if it.len() <= 0 {
                Ok::<_, anyhow::Error>(Cow::Borrowed(text))
            } else {
                let mut new = String::with_capacity(text.len());
                let mut last_match = 0;
                for (i, cap) in it {
                    if limit > 0 && i >= limit {
                        break;
                    }
                    // unwrap on 0 is OK because captures only reports matches
                    let m = cap.get(0).unwrap();
                    new.push_str(&text[last_match..m.start()]);
                    // NOTE: unfortunately can't parallelize this since state is dependent on previous iterations...
                    // TODO: Work a little harder to figure out a way to parallelize this
                    new.push_str(
                        self.clone()
                            ._style_regex_replacer(&cap, force, tx.clone())
                            .await?
                            .as_ref(),
                    );
                    last_match = m.end();
                }
                new.push_str(&text[last_match..]);
                Ok(Cow::Owned(new))
            }
        }?;

        if let Some(p) = out_path.parent() {
            tokio::fs::create_dir_all(p).await?;
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
        let mut f = File::create(&out_path).await?;
        f.write_all(minified_css.as_bytes()).await?;

        event!(Level::INFO, r#type = "new", path = ?out_path);

        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self), name = "process_font")]
    async fn render_font(
        self: Arc<Self>,
        input: RenderingInput,
        force: bool,
    ) -> anyhow::Result<()> {
        // Just download the file to the given path
        let (url, output) = match input {
            RenderingInput::Font {
                ref input,
                ref output,
            } => (input, output),
            _ => panic!("Expected font"),
        };
        let out_path = self.config.roots.output.join("fonts").join(output);

        if !force && tokio::fs::metadata(&out_path).await.is_ok() {
            event!(Level::INFO, r#type = "fresh", %url);
            return Ok(());
        }

        let mut r = self
            .client
            .get(url.as_str())
            .send()
            .await
            .map_err(|_| anyhow::anyhow!("fetch failed"))?
            .compat();
        if let Some(parent) = out_path.parent() {
            tokio::fs::create_dir_all(parent).await?;
        }
        let mut f = File::create(&out_path).await?;
        tokio::io::copy(&mut r, &mut f).await?;

        event!(Level::INFO, r#type = "new", path = ?out_path);

        Ok(())
    }

    #[instrument(level = Level::INFO, skip(self))]
    async fn render(
        self: Arc<Self>,
        input: RenderingInput,
        force: bool,
        tx: UnboundedSender<anyhow::Result<()>>,
    ) -> anyhow::Result<()> {
        let out_dir = &self.config.roots.output;
        let base_dir = &self.config.roots.source;
        let style_chunks_root = &self.config.lib.styles.chunks_root;
        let prelude_html = &self.config.lib.prelude_location;
        let filename = match input {
            RenderingInput::Index => &self.config.inputs.index,
            RenderingInput::Keep => &self.config.inputs.keep,
            RenderingInput::Style(..) => return self.render_style(input, force, tx).await,
            RenderingInput::Font { .. } => return self.render_font(input, force).await,
            RenderingInput::Image { .. } => return self.render_image(input, force).await,
            RenderingInput::Page(ref o) => o,
        };

        if !filename.exists() {
            event!(Level::INFO, r#type = "nonexistent_source", path = ?filename);
            return Ok(());
        }

        // create out dir if doesn't exist
        if !out_dir.exists() {
            tokio::fs::create_dir_all(out_dir).await?;
        }

        // NOTE: can't canonicalize here since the output path may not exist
        let out_path = out_dir
            .join(filename.strip_prefix(&base_dir)?)
            .with_extension("html");

        let buf = {
            let mut s = String::new();
            let mut f = File::open(&filename).await?;
            f.read_to_string(&mut s).await?;
            Ok::<_, std::io::Error>(s)
        }?;

        let mut styles = {
            let mut h = HashSet::new();
            h.insert("_global");
            h
        };

        let (html, frontmatter) = {
            /* No awaits from here... */

            let parser = Parser::new_ext(&buf, Options::all());
            let mut new_stack = Vec::new();
            let mut ctx = ProcessorContext {
                filename,
                styles: &mut styles,
                config: &self.config,
                finished: &self.finished,
                render_stack: &self.render_stack,
                new_stack: &mut new_stack,
                ss: &self.ss,
                theme: &self.ts.themes[&self.config.theme],
            };
            let mut adapter = RenderAdapter::new(parser, &mut ctx);

            let mut s = String::new();
            html::push_html(&mut s, &mut adapter);

            s = adapter.postprocess_syntax_highlighting(&s);
            s = adapter.setup_header_links(&s);

            let toc = adapter.render_toc();
            s = format!("{}{}", toc, s);

            let fm = adapter.frontmatter.take();
            /* ...to here. */

            for input in new_stack {
                self.clone().spawn_input(force, input, tx.clone());
            }

            (s, fm)
        };
        let frontmatter = frontmatter.unwrap_or_else(|| crate::frontmatter::Frontmatter {
            title: "Untitled".to_string(),
            date: None,
            time_to_read: None,
        });

        let styles = {
            let mut new_styles = Vec::new();
            for sname in styles.into_iter() {
                let path = style_chunks_root.join(sname).with_extension("css");
                // skip missing files
                if let Ok(_) = AsRef::<Path>::as_ref(&path).canonicalize() {
                    let css_out_path = out_dir.join("css").join(sname).with_extension("css");
                    let input = RenderingInput::Style(sname);
                    if !self.render_stack.contains(&input) && !self.finished.contains(&input) {
                        self.render_stack.insert(input.clone());
                        self.clone().spawn_input(force, input, tx.clone());
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
            let mut f = File::open(prelude_html).await?;
            let mut s = String::new();
            f.read_to_string(&mut s).await?;
            Ok::<_, std::io::Error>(s)
        }?
        .replace("@@@SLOT_STYLES@@@", &format!("\n{}\n", styles.join("\n")))
        .replace("@@@SLOT_CONTENT@@@", &html)
        .replace("@@@SLOT_TITLE@@@", &frontmatter.title);

        let html = {
            let mut html = html;

            let date_r = RegexBuilder::new(r#"<!-- @@@IF_DATE@@@ -->(.*?)<!-- @@@ENDIF@@@ -->"#)
                .dot_matches_new_line(true)
                .build()
                .unwrap();
            if let Some(d) = frontmatter.date {
                html = date_r
                    .replace_all(&html, |caps: &Captures| {
                        // Expand dates inside and all that
                        let inner = &caps[1];
                        let date = d.format(DATE_FORMAT).to_string();
                        inner.replace("@@@SLOT_DATE@@@", &date)
                    })
                    .to_string();
            } else {
                html = date_r.replace_all(&html, "").to_string();
            }
            let ttr_r =
                RegexBuilder::new(r#"<!-- @@@IF_TIME_TO_READ@@@ -->(.*?)<!-- @@@ENDIF@@@ -->"#)
                    .dot_matches_new_line(true)
                    .build()
                    .unwrap();
            if let Some(ttr) = frontmatter.time_to_read {
                html = ttr_r
                    .replace_all(&html, |caps: &Captures| {
                        // Expand ttr inside and all that
                        let inner = &caps[1];
                        inner.replace("@@@SLOT_TIME_TO_READ@@@", &ttr)
                    })
                    .to_string();
            } else {
                html = ttr_r.replace_all(&html, "").to_string();
            }

            html
        };

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
        let needs_update = if let (Ok(out_metadata), Ok(in_metadata)) = (
            tokio::fs::metadata(&out_path).await,
            tokio::fs::metadata(&filename).await,
        ) {
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
                tokio::fs::create_dir_all(p).await?;
            }

            if input == RenderingInput::Keep {
                event!(Level::INFO, r#type = "special_keep", path = ?out_path);
            } else {
                let mut f = File::create(&out_path).await?;
                f.write_all(minified.as_bytes()).await?;
                // println!("{}", html);
                event!(Level::INFO, r#type = "new", path = ?out_path);
            }
        }

        Ok(())
    }
}
