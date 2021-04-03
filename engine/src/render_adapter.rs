use std::{
    collections::{HashMap, HashSet},
    path::{Path, PathBuf},
};

use dashmap::DashSet;
use pulldown_cmark::{escape, Event, LinkType, Tag};
use regex::{Captures, Regex, RegexBuilder};
use syntect::{highlighting::Theme, parsing::SyntaxSet};
use tracing::{event, instrument, Level};
use url::Url;

use crate::process::RenderingInput;
use crate::{config::ResolvedConfig, frontmatter::Frontmatter};

pub struct RenderAdapter<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> {
    ctx: &'a mut ProcessorContext<'a, 'c>,
    iter: I,
    // Table of contents
    // level, title, slug
    toc: Vec<(usize, String, String)>,
    // Cache for header slugification
    slugs_cache: HashMap<String, usize>,
    // Extracted and parsed front matter, if any
    pub(crate) frontmatter: Option<Frontmatter>,
    // Frontmatter parsing state
    frontmatter_state: FrontmatterParsingState,
}

#[derive(Debug)]
enum FrontmatterParsingState {
    // Waiting for frontmatter
    Ready,
    // Currently parsing frontmatter
    Parsing(String),
    // Done parsing frontmatter
    Done,
}

const TOC_START: &'static str = r#"
<section class="toc">
    <h1>Table of contents</h1>
"#;

const TOC_END: &'static str = r#"
</section>
"#;

impl<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> RenderAdapter<'a, 'b, 'c, I> {
    pub fn new(iter: I, ctx: &'a mut ProcessorContext<'a, 'c>) -> Self {
        Self {
            iter,
            ctx,
            toc: Vec::new(),
            slugs_cache: HashMap::new(),
            frontmatter: None,
            frontmatter_state: FrontmatterParsingState::Ready,
        }
    }

    // Converts a header title into a slug.
    fn header_slug(&mut self, title: &str) -> String {
        let fixed_up = title
            .to_lowercase()
            .replace(" ", "-")
            .replace(|c: char| !c.is_alphanumeric() && c != '-', "");
        if self.slugs_cache.contains_key(&fixed_up) {
            self.slugs_cache
                .insert(fixed_up.clone(), self.slugs_cache[&fixed_up] + 1);
            format!("{}{}", fixed_up, self.slugs_cache[&fixed_up])
        } else {
            self.slugs_cache.insert(fixed_up.clone(), 0);
            format!("{}", fixed_up)
        }
    }

    /// Post processes syntax highlighting for code blocks
    /// and adds "code" to styles if necessary
    pub fn postprocess_syntax_highlighting(&mut self, inp: &str) -> String {
        let r = RegexBuilder::new(r#"<pre><code class="language-([^\n]+?)">(.*?)</code></pre>"#)
            .dot_matches_new_line(true)
            .build()
            .unwrap();
        let r2 = Regex::new(r#"<pre(.*)>\n"#).unwrap();
        let ss = self.ctx.ss;
        let theme = self.ctx.theme;
        r.replace_all(inp, |caps: &Captures| {
            self.ctx.styles.insert("code");
            let language_token = &caps[1];
            let text = &caps[2]
                .replace("&lt;", "<")
                .replace("&gt;", ">")
                .replace("&quot;", "\"");
            let syntax = ss
                .find_syntax_by_token(language_token)
                .unwrap_or_else(|| ss.find_syntax_plain_text());
            let highlighted = syntect::html::highlighted_html_for_string(text, &ss, syntax, theme);
            let highlighted = r2
                .replace_all(&highlighted, |caps: &Captures| {
                    format!(
                        r#"<pre{0}><code class="language-{1}">"#,
                        &caps[1], language_token
                    )
                })
                .replace("</pre>", "</code></pre>");
            highlighted
        })
        .into_owned()
    }

    /// Sets up header links so that the TOC can be generated.
    pub fn setup_header_links(&mut self, inp: &str) -> String {
        let r = Regex::new(r"<h(\d)>(.*?)</h\d>").unwrap();
        r.replace_all(inp, |caps: &Captures| {
            let level = caps[1]
                .parse::<usize>()
                .expect("Only numbers can be parsed here");
            let text = &caps[2];
            let slug = self.header_slug(&text);
            self.toc.push((level, text.to_string(), slug.clone()));
            format!(r#"<h{0} id="{1}">{2}</h{0}>"#, level, slug, text)
        })
        .into_owned()
    }

    /// Renders the table of contents
    /// and adds "toc" to the styles if necessary
    pub fn render_toc(&mut self) -> String {
        if self.toc.is_empty() {
            return String::new();
        }
        self.ctx.styles.insert("toc");
        self.ctx.styles.insert("link");
        let mut s = String::new();
        s.push_str(TOC_START);
        let mut last_level = 0;
        for (level, title, slug) in std::mem::take(&mut self.toc) {
            if level > last_level {
                s.push_str("<ol>");
            }
            if level < last_level {
                s.push_str("</ol>");
            }
            let escaped_slug = {
                let mut escaped = String::new();
                escape::escape_href(&mut escaped, &slug).unwrap();
                escaped
            };
            let escaped_title = {
                let mut escaped = String::new();
                escape::escape_html(&mut escaped, &title).unwrap();
                escaped
            };
            s.push_str(&format!(
                "<li><a href=\"#{}\">{}</a></li>",
                escaped_slug, escaped_title
            ));
            last_level = level;
        }
        for _ in 0..last_level {
            s.push_str("</ol>");
        }
        s.push_str(TOC_END);
        s
    }
}

impl<'a, 'b, 'c: 'a, I: Iterator<Item = Event<'b>>> Iterator for RenderAdapter<'a, 'b, 'c, I> {
    type Item = Event<'b>;

    #[instrument(name = "process", skip(self))]
    fn next(&mut self) -> Option<Self::Item> {
        let mut item = self.iter.next()?;
        let styles = &mut self.ctx.styles;
        let new_stack = &mut *self.ctx.new_stack;
        let render_stack = self.ctx.render_stack;
        let finished = self.ctx.finished;
        let out_dir = &self.ctx.config.roots.output;
        let base_dir = &self.ctx.config.roots.source;
        let filename = self.ctx.filename;
        if let Event::Rule = item {
            // Start frontmatter parsing
            use FrontmatterParsingState::*;
            if let Ready = self.frontmatter_state {
                // println!("starting frontmatter parsing");
                self.frontmatter_state = Parsing(String::new());
            } else if let Parsing(ref s) = self.frontmatter_state {
                // Finish parsing
                if !s.is_empty() {
                    // println!("Parsing front matter: {}", s);
                    let r = Frontmatter::parse_from_str(&s);
                    match r {
                        Ok(r) => {
                            println!("Parsed front matter: {:#?}", r);
                            self.frontmatter = Some(r);
                        }
                        Err(e) => {
                            println!("Error parsing front matter: {}", e);
                            self.frontmatter = None;
                        }
                    }
                }
                self.frontmatter_state = Done;
            }
        }
        if let Event::End(Tag::Heading(..)) = item {
            use FrontmatterParsingState::*;
            // End frontmatter parsing if needed
            if let Parsing(ref s) = self.frontmatter_state {
                // Finish parsing
                if !s.is_empty() {
                    // println!("Parsing front matter: {}", s);
                    let r = Frontmatter::parse_from_str(&s);
                    match r {
                        Ok(r) => {
                            println!("Parsed front matter: {:#?}", r);
                            self.frontmatter = Some(r);
                        }
                        Err(e) => {
                            println!("Error parsing front matter: {}", e);
                            self.frontmatter = None;
                        }
                    }
                }
                self.frontmatter_state = Done;
            }
        }
        if let Event::Text(ref s) = item {
            if let FrontmatterParsingState::Parsing(ref mut ps) = self.frontmatter_state {
                // println!("Frontmatter parsing got {}", s);
                ps.push_str(s);
            }
        }
        if let Event::SoftBreak = item {
            if let FrontmatterParsingState::Parsing(ref mut ps) = self.frontmatter_state {
                ps.push('\n');
            }
        }
        if let FrontmatterParsingState::Parsing(..) = self.frontmatter_state {
            // Skip this element since front matter is being parsed
            // This should eventually lead to the parsing ending.. therefore element get emitted
            // TODO: does this blow the stack?
            return self.next();
        }
        if let Event::Start(Tag::Image(..)) = item {
            styles.insert("image");
        }
        if let Event::Start(Tag::Image(LinkType::Inline, ref mut url, _)) = item {
            if let Ok(parsed) = Url::parse(&url) {
                use sha2::Digest;
                let hashname = format!("{:x}", sha2::Sha256::digest(parsed.as_str().as_bytes()));
                let new_url = format!("/images/{}.webp", hashname);
                let input = RenderingInput::Image {
                    input: parsed,
                    output: hashname,
                };
                if !render_stack.contains(&input) && !finished.contains(&input) {
                    render_stack.insert(input.clone());
                    new_stack.push(input);
                }
                *url = new_url.into();
            }
        }
        if let Event::Start(Tag::Paragraph) = item {
            styles.insert("paragraph");
        }
        if let Event::Start(Tag::Heading(level)) = item {
            match level {
                1 => {
                    styles.insert("h1");
                }
                _ => {}
            }
        }
        if let Event::Start(Tag::Link(..)) = item {
            styles.insert("link");
        }
        if let Event::Start(Tag::Link(LinkType::Inline, ref mut url, _)) = item {
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
                        let input = RenderingInput::Page(fname);
                        if !render_stack.contains(&input) && !finished.contains(&input) {
                            match input {
                                RenderingInput::Page(ref fname) => {
                                    event!(Level::INFO, r#type = "walk", ?fname)
                                }
                                _ => {}
                            }
                            render_stack.insert(input.clone());
                            new_stack.push(input);
                        }
                        *url = new_location.into();
                    } else {
                        event!(Level::WARN, r#type = "invalid_hyperref", %url);
                    }
                }
            }
        }
        Some(item)
    }
}

/// Processing context for a single file
pub struct ProcessorContext<'a, 'b: 'a> {
    pub(crate) styles: &'a mut HashSet<&'b str>,
    pub(crate) filename: &'a Path,
    pub(crate) config: &'a ResolvedConfig,
    pub(crate) finished: &'a DashSet<RenderingInput>,
    pub(crate) render_stack: &'a DashSet<RenderingInput>,
    pub(crate) new_stack: &'a mut Vec<RenderingInput>,
    pub(crate) ss: &'a SyntaxSet,
    pub(crate) theme: &'a Theme,
}
