# Getting Started

Three ways in: a koi on your profile README, the command line, and the library.
Each stands alone.

## On Your Profile README

GitHub shows the README of the repository named after your username, such as
`octocat/octocat`, on your profile page. To give it a koi that redraws every
night:

1. Copy [examples/profile-workflow.yml](../examples/profile-workflow.yml) into
   that repository as `.github/workflows/koi.yml`.
2. Embed the koi in its `README.md`:

   ```html
   <img src="koi/koi.gif" width="100%" alt="My koi" />
   ```

   The GIF is a wide 1536 by 512 strip drawn at twice the koi's pixel size, and
   the koi wanders its whole width, so `width="100%"` lets it fill the README.

3. Run the workflow once from the repository's Actions tab, so the first koi
   appears without waiting for midnight.

The workflow runs just after midnight UTC, so each koi shows the day that just
ended, and again whenever the workflow file changes. Its `git-koi` step is this
repository's Action. It collects your public activity with the workflow's
built-in token and renders `koi.gif`, `koi.png`, and `preview.html` into `koi/`.
The Action never commits. The workflow's last step commits `koi/koi.gif` only
when it changed, so an unchanged koi adds no commit.

| Input        | Default              | Meaning                                                   |
| ------------ | -------------------- | --------------------------------------------------------- |
| `login`      | The repository owner | Account whose koi to draw                                 |
| `body`       | `white`              | `white`, `black`, `asagi`, `tea`, `gold`, or `platinum`   |
| `output-dir` | `koi`                | Folder that receives `koi.gif`, `koi.png`, `preview.html` |
| `token`      | The built-in token   | Token for reading the account's public data               |

To change the body, edit `body` in your copy of the workflow; pushing the change
runs the workflow and redraws the koi. If you change `output-dir`, change the
commit step's path and the README image to match. The example uses
`yalongwastaken/git-koi@v1`, which moves to each compatible release; pin a full
release tag to choose when you update.

GitHub pauses scheduled workflows in a public repository after 60 days without
activity such as a push. A koi that stops changing during a long quiet stretch
adds no commits, so its schedule can pause and does not resume on its own.
Re-enable it from the Actions tab or with `gh workflow enable koi.yml`.

## From the Command Line

Install one of two ways. With Rust at or above the `rust-version` in
[Cargo.toml](../Cargo.toml):

```sh
cargo install git-koi --locked
```

Or download the archive for your platform from the
[releases page](https://github.com/yalongwastaken/git-koi/releases), a `.tar.gz`
for Linux and macOS or a `.zip` for Windows. Each archive has a `.sha256` file
beside it; check the download with `shasum -a 256 -c` on macOS, `sha256sum -c`
on Linux, or `certutil -hashfile` on Windows, then put `git-koi` on your `PATH`.
macOS may refuse to open a binary it downloaded;
`xattr -d com.apple.quarantine git-koi` clears that flag.

Collection goes through the [GitHub CLI](https://cli.github.com), so install it
and sign in once with `gh auth login`. Then:

```sh
git-koi collect octocat --output snapshot.json
git-koi render snapshot.json --output-dir koi --body asagi
```

Open `koi/preview.html` in a browser to see the GIF and the still on GitHub's
light and dark page colors. `--body` takes `white`, the default, `black`,
`asagi`, `tea`, `gold`, or `platinum`.

`collect` reads only public data: the account's public repositories, the public
repositories it has contributed to, GitHub's color for each language, and its
daily contributions from the account's creation through the last complete UTC
day. It writes the snapshot to standard output, or with `--output` replaces the
file only after collection succeeds. In GitHub Actions, `GH_TOKEN` set to the
workflow's token is enough. `render` draws everything in memory first and
replaces each file through a temporary file, so a failed run keeps the previous
files. Both print errors to standard error and exit 1; invalid usage exits 2.
`git-koi --help` prints the full usage and `git-koi --version` the version.

## As a Library

The same crate is a library, documented on [docs.rs](https://docs.rs/git-koi).
Add it beside `serde_json`, which reads the snapshot:

```sh
cargo add git-koi serde_json
```

The pipeline runs one way. Read or collect an `AccountSnapshot`, derive the
koi's development, choose the day's behavior, render frames, and encode them:

```rust
use git_koi::behavior::select_behavior;
use git_koi::coat::KoiBody;
use git_koi::history::AccountSnapshot;
use git_koi::koi::derive_koi_development;
use git_koi::motion::frame_count;
use git_koi::output::{encode_gif, encode_png};
use git_koi::render::{render_koi, render_swim_frame};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let snapshot_json = std::fs::read_to_string("snapshot.json")?;
    let account_snapshot: AccountSnapshot = serde_json::from_str(&snapshot_json)?;
    let koi_development = derive_koi_development(&account_snapshot)?;
    let koi_body = KoiBody::Asagi;
    let behavior = select_behavior(&koi_development);
    let swim_frames: Vec<_> = (0..frame_count(behavior))
        .map(|frame_index| render_swim_frame(&koi_development, koi_body, behavior, frame_index))
        .collect();
    std::fs::write("koi.gif", encode_gif(&swim_frames)?)?;
    std::fs::write("koi.png", encode_png(&render_koi(&koi_development, koi_body))?)?;
    Ok(())
}
```

To collect in-process instead of reading a file, call
`collect::collect_account_snapshot` with `collect::GhCli` and today's UTC date.
Everything after collection is deterministic and performs no I/O, so the same
snapshot always draws the same koi. Breaking changes to the API bump the major
version.
