---
title: Code-heavy post
tags: [code, rust, python]
---

Some code in several languages.

```rust
use std::collections::HashMap;

/// A doc comment.
fn main() {
    let mut m: HashMap<&str, i32> = HashMap::new();
    m.insert("answer", 42);
    println!("{:?}", m.get("answer")); // a comment
}
```

```python
def greet(name: str) -> str:
    """Docstring."""
    return f"Hello, {name}!"
```

```js
const xs = [1, 2, 3].map((x) => x * 2);
console.log(`doubled: ${xs}`);
```

```
A code block with no language and a very long line that should scroll horizontally instead of wrapping or overflowing the page container: 0123456789 0123456789 0123456789 0123456789
```

    An indented code block.

```html
<p class="x">Escaped &amp; highlighted HTML</p>
```
