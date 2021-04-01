use std::{
    collections::HashSet,
    path::{Path, PathBuf},
};

use pulldown_cmark::{Event, LinkType, Tag};
use tracing::{event, instrument, Level};
use url::Url;

use crate::config::ResolvedConfig;
use crate::process::RenderingInput;

pub struct RenderAdapter<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> {
    pub(crate) ctx: &'a mut ProcessorContext<'a, 'c>,
    pub(crate) iter: I,
}

impl<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> Iterator for RenderAdapter<'a, 'b, 'c, I> {
    type Item = Event<'b>;

    #[instrument(name = "process", skip(self))]
    fn next(&mut self) -> Option<Self::Item> {
        let item = self.iter.next()?;
        let styles = &mut self.ctx.styles;
        let new_stack = &mut *self.ctx.new_stack;
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
                                    render_stack.insert(input.clone());
                                    new_stack.push(input);
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
                                            render_stack.insert(input.clone());
                                            new_stack.push(input);
                                        }
                                        Event::Start(Tag::Link(ty, new_location.into(), title))
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
    pub(crate) styles: &'a mut HashSet<&'b str>,
    pub(crate) filename: &'a Path,
    pub(crate) config: &'a ResolvedConfig,
    pub(crate) finished: &'a HashSet<RenderingInput>,
    pub(crate) render_stack: &'a mut HashSet<RenderingInput>,
    pub(crate) new_stack: &'a mut Vec<RenderingInput>,
}
