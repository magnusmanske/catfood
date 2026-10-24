
# Hard rules
- **Do not mention AI agents in commit messages.**
- **`git commit` is pre-authorized; `git push` is not.** Use sensible commit groupings for larger changes.
- ** Keep security in mind**, especially if this is a web-facing product.
- Always keep **code readability and long-term maintenance** in mind. Humans work on this code as well, so keep code and comments succinct and clear.
- **Keep the code simple** and elegant.
- **Aim to keep code small** where possible.
- Adhere to **SOLID and DRY principles**.
- Use **best practices** and **language standards**.
- **Take advantage of the Rust language**. "Parse, don't validate" etc.
- **No unsafe code**. Only use safe Rust.
- **Write code that does not panic**. Avoid panics in production code. Return proper `Result` errors instead.
- **Write tests** where it makes sense. Aim to keep test coverage high and tests simple.
- **Warn me if you think what I ask of you is a bad idea.** Or just window dressing, with no improvement of functionality, UX, or code readability. Be honest.
- **Fix clippy warnings**, even pre-existing ones.
