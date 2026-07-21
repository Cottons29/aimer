# Introducing Markdown Renderer

One of the goals behind Aimer has always been to make building rich, content-heavy UIs as painless as building simple
ones. A common need across documentation viewers, note-taking apps, and chat interfaces is rendering **Markdown**
directly into the widget tree — without dropping down to raw HTML or a separate embedded browser view. This post
introduces Aimer's built-in Markdown renderer.

### Why a Native Markdown Renderer?

Most GUI frameworks either lack Markdown support entirely or rely on rendering it as HTML inside a webview, which pulls
in a heavyweight dependency and breaks visual consistency with the rest of the app. Aimer's Markdown renderer instead
parses Markdown directly into native Aimer widgets, so headings, lists, code blocks, and inline styles are rendered
using the same layout and styling system as everything else in your app — fully themeable and GPU-accelerated like any
other Aimer widget tree. currently Aimer have supported the basic Markdown syntax.

### Example Snippet

```rust
const MARKDOWN_SOURCE = include_str!("my_markdown.md");

#[aimer::main]
pub fn start_app() {
    AimerApp::start(
      Container::new()
        .child(
          MarkdownViewer::new()
            .padding(LayoutSpacing::all(Spacing::Px(16)))
            .theme(MarkdownTheme::default())
            .markdown(MARKDOWN_SOURCE)
          )
    );
}
```

This renders a fully styled document — headings, bold and italic text, and a bulleted list — directly as Aimer widgets,
no webview required.

# Supported Syntax

````markdown
# Headings
# H1
## H2
### H3
#### H4
##### H5
###### H6

# Emphasis
*italic* or _italic_
**bold** or __bold__
***bold italic***
~~strikethrough~~

# Lists
- Unordered item
* Also unordered
+ Also unordered
1. Ordered item
2. Ordered item

# Task Lists
- [x] Completed task
- [ ] Incomplete task

# Links
[link text](https://example.com)
[link text](https://example.com "optional title")
<https://example.com>  (autolink)

# Images
![alt text](image.jpg)
![alt text](image line here)
![alt text](image.jpg "optional title")

# Blockquotes
> This is a quote
>> Nested quote

# Code
`inline code`

```
fenced code block
```

```python
# fenced code block with syntax highlighting
```
    indented code block (4 spaces)

# Horizontal Rule
---
***
___

# Line Breaks
Paragraph one.

Paragraph two (blank line = new paragraph).

Line one  
Line two (two trailing spaces = line break)

# Tables
| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
| Cell 3   | Cell 4   |

# Footnotes
Here is a footnote reference[^1].

[^1]: Footnote text.

# Escaping Characters
\* not italic \*

````

# Markdown Render Showcase


# Headings
# H1
## H2
### H3
#### H4
##### H5
###### H6

# Emphasis
*italic* or _italic_
**bold** or __bold__
***bold italic***
~~strikethrough~~

# Lists
- Unordered item
* Also unordered
+ Also unordered
1. Ordered item
2. Ordered item

# Task Lists
- [x] Completed task
- [ ] Incomplete task

# Links
[link text](https://example.com)

[link text](https://example.com "optional title")

<https://example.com>  (autolink)


# Blockquotes
> This is a quote
>> Nested quote


# Blockquotes With Other Elements
> # This is a quote
>> ### Nested quote

# Code
`inline code`

```
fenced code block
```

```python
# fenced code block
print("Hello World")
```
    indented code block (4 spaces)

# Horizontal Rule
---
***
___

# Line Breaks
Paragraph one.

Paragraph two (blank line = new paragraph).

Line one  
Line two (two trailing spaces = line break)

# Tables
| Header 1 | Header 2 |
|----------|----------|
| Cell 1   | Cell 2   |
| Cell 3   | Cell 4   |

# Footnotes
Here is a footnote reference [^1].

[^1]: Footnote text.

# Escaping Characters
\* not italic \*

\*\*not bold too\*\*


## Credit

- **arborium** crate for syntax parser
- **Jetbrains** for `JetBrainsMono-2.304`