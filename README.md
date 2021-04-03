# [anirudhb.github.io](https://anirudhb.github.io)

This is v2 of my personal website!!

[![Lighthouse 100%](https://img.shields.io/badge/lighthouse-100%25-brightgreen)](https://developers.google.com/speed/pagespeed/insights/?url=https%3A%2F%2Fanirudhb.github.io)

## Site generated using engine

To generate:

```
cd engine/
cargo run -- ../config.toml
```

`--force` can be used to force a regeneration of all files.

Output is in out/

# Using the engine

Feel free to use my engine for your own websites :)
Note that the engine is licensed under AGPLv3 (my website is all rights reserved.)

The engine processes Markdown files into generated HTML that can be served as a static site.

## Configuration

To get started create a `config.toml` file that specifies some locations such as the location of styles, sources and outputs:

```toml
# Engine config
[roots]                                        # required
source = "src"                                 # required
lib = "lib"                                    # required
assets = "assets"                              # required
output = "out"                                 # required
theme = "Monokai"                              # optional

[inputs]                                       # optional
index = "${roots.source}/index.md"             # optional
keep = "${roots.source}/_keep.md"              # optional

[lib]                                          # optional
prelude_location = "${roots.lib}/prelude.html" # optional
# theme_location = "mythemes/"                 # optional
[lib.styles]                                   # optional
chunks_root = "${roots.lib}/style-chunks"      # optional
# relative filenames here are resolved relative
# to ${lib.styles.chunks_root}
# map of style names to filenames
[lib.styles.css]                               # optional
# defaults
# global = "_global.css"
# * = "*.css"
```

## Usage

Now you're ready to start writing your website from `${inputs.index}`!
The defaults above are pretty sane so feel free to use them as a template.

### Hyperref

To reference other pages, **do not** use normal paths like `/blog.html` or `blog.html`.
These **will not work**!
Instead, use `hyperref:blog` or `hyperref:/blog` or `hyperref:blog.md`.
Using the special `hyperref` scheme tells the engine that the corresponding page is used (linked to from some other used page.)
This is used to build a dependency tree and prevents unnecessary processing (also see [Using the keep file](#using-the-keep-file).)

### Image optimization

Any images included in your Markdown files will automatically be optimized<sup>1</sup> and statically fetched at build time.
To prevent this, you can use the `-noprocess` suffix to URL schemes.
This is useful if you want to link to some kind of dynamic image, thought it may impact your [Lighthouse score](#lighthouse).
For example, to prevent `https://example.com/image.png` from being optimized:

```markdown
![Don't optimize me!](https-noprocess://example.com/image.png)
```

Note: Except for SVGs, all other image formats are automatically converted to WebP.

### Image assets

Assets can be linked using the special `asset:` scheme.
This functions similarly to the `hyperref:` scheme.
Note that assets will always be optimized, and this behavior cannot be disabled.
For example, to link to `assets/image5.png`:

```
![Optimized asset](asset:image5.png)
```

### Styling

Styles are automatically added based on necessity.
The styles are looked up by the `${lib.styles.css}` map from the config and resolved relative to `${lib.styles.chunks_root}`.
The global style name defaults to `_global.css`, and any other style names default to the name with the `.css` extension added (e.g. `image` -> `image.css`.)

### Font optimization

Often times you would like to include webfonts.
However, this is shown to negatively impact [Lighthouse scores](#lighthouse). Engine automatically optimizes fonts if they are added properly, keeping your Lighthouse score high.
To optimize a font, remove the `link` tag from your prelude and add a special CSS comment to your stylesheet:

```css
/** @font https://fonts.googleapis.com/css2?family=Roboto&display=swap */
```

Font optimization will fetch the stylesheet and embed it inline. Any font files it references will also be converted into static assets.
This is especially beneficial when using HTTP/2 since latency is lower on first-party fetches than on external sites.

### Syntax highlighting

engine includes syntax highlighting for code blocks by default.
If a theme directory is specified in the config, additional TextMate themes (\*.tmTheme) will be loaded from the directory.
By default, engine uses the `Monokai` theme, but this can be configured.

Additionally, engine comes with the following built-in themes:

- `Monokai`
- `Visual Studio Code Dark+`

The following languages are not currently supported for syntax highlighting but will be supported in the future:

- TypeScript

### Frontmatter parsing

engine parses front matter by default.
Frontmatter can be specified as such at the beginning of a Markdown file:

```yaml
---
title: My title
date: 04/03/2021
time_to_read: 17 minutes
---

```

The current fields that are parsed are:

- Title (string, required)
- Date (`MM/DD/YYYY` format, optional)
- Time to read (string, optional)

**Note**: In YAML, the absence of a field does not make it null.
Therefore, to specify that a field is null, use `~` or `null` as the value, like this:

```yaml
---
title: My title
date: ~ # No date
time_to_read: 5 seconds
---

```

Otherwise the frontmatter will not parse!

Additionally, **note that frontmatter is required.**
A title is required at minimum.

### Using the keep file

The keep file (`${inputs.keep}`) is a special file which will never be written to the output folder.
If you would like to render "hidden" pages (i.e. those that are not linked to), you can use the keep file for that.
Anything linked to by the keep file will be rendered as normal, but the keep file itself will not be.
For example, if you wanted to keep the hidden page `secret.md`:

```markdown
[](hyperref:secret)
```

### Prelude

The prelude file (`${roots.lib.prelude_location}`) is a file that acts as an HTML template for all of your pages.
There are two important "slots" that must be present in the prelude: content and styles.
Additionally, frontmatter properties can be used if available.

To add a slot to your prelude, simply write:

```html
<!-- Your content goes here -->
@@@SLOT_CONTENT@@@
<!-- Your styles go here -->
@@@SLOT_STYLES@@@

<!-- The title will go here -->
@@@SLOT_TITLE@@@

<!-- This section will be included if there is a date -->
<!-- @@@IF_DATE@@@ -->
<time>@@@DATE@@@</time>
<!-- @@@ENDIF@@@ -->

<!-- This section will be included if there is a time to read -->
<!-- @@@IF_TIME_TO_READ@@@ -->
<time>@@@IF_TIME_TO_READ@@@</time>
<!-- @@@ENDIF@@@ -->
```

### Lighthouse

[Lighthouse](https://developers.google.com/web/tools/lighthouse) is a tool which measures the performance of your website.
With a few tweaks and sane defaults, engine will get 100% on Lighthouse.
Engine uses a few strategies to speed up your website, such as image optimization and font optimization.

However, some of the audits that Lighthouse performs cannot be automatically fixed by the engine.
It is up to you to fix them by adjusting the prelude and stylesheets.

<sup>1</sup> Images are automatically optimized into WebP if they are not already. SVGs are not supported (for now)

## Engine: to do

- [ ] Implement `https-noprocess`
- [x] Table of contents generator
- [ ] Fragment inclusion (e.g. JavaScript)
- [ ] Actually figure out how to do generated fragments (hard)
- [ ] Actually implement image assets
- [ ] `/** @include */` for CSS?
