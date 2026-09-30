#!/usr/bin/env bash
# End-to-end: `watch` over a temp root with two blogs deploying to local bare git repos.
#  1. Drop a Markdown file into one blog: the post and updated feed reach its bare repo,
#     the other blog's repo is untouched.
#  2. Break a template: the deployed output is unchanged and `status` shows the failure.
#  3. Fix it: the pending post deploys.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build -q
BIN="$PWD/target/debug/folderblog"
export FOLDERBLOG_NO_NOTIFY=1
T="$(mktemp -d)"
ROOT="$T/Blogs"
trap 'kill $WATCH_PID 2>/dev/null || true; rm -rf "$T"' EXIT

step() { printf '\n== %s\n' "$*"; }
wait_for() { # wait_for <seconds> <description> <command...>
  local secs=$1 desc=$2; shift 2
  for _ in $(seq 1 $((secs * 5))); do "$@" >/dev/null 2>&1 && return 0; sleep 0.2; done
  echo "TIMEOUT waiting for: $desc"; echo "--- watch log"; cat "$T/watch.log"; exit 1
}
head_of() { git --git-dir="$T/$1.git" rev-parse --verify -q gh-pages; }
show() { git --git-dir="$T/$1.git" show "gh-pages:$2"; }

step "create two blogs, each deploying to its own bare repo"
for b in alpha.test beta.test; do
  "$BIN" new "$b" --root "$ROOT" >/dev/null
  git init -q --bare "$T/$b.git"
  sed -i "s|^base_url = .*|base_url = \"https://$b\"|" "$ROOT/$b/blog.toml"
  printf '\n[deploy]\ntype = "git"\nremote = "%s"\nbranch = "gh-pages"\n' "$T/$b.git" >> "$ROOT/$b/blog.toml"
  # blog.toml ends with an empty [params] table; move deploy above it so params stays last
  python3 - "$ROOT/$b/blog.toml" <<'EOF'
import sys; p=sys.argv[1]; s=open(p).read()
s=s.replace("[params]\n","",1)+"\n[params]\n"; open(p,"w").write(s)
EOF
done
printf '# Beta first post\n\nBeta content.\n' > "$ROOT/beta.test/posts/beta-first.md"

step "start watch"
"$BIN" watch --root "$ROOT" > "$T/watch.log" 2>&1 &
WATCH_PID=$!
wait_for 30 "initial deploy of alpha" head_of alpha.test
wait_for 30 "initial deploy of beta" head_of beta.test
A0=$(head_of alpha.test); B0=$(head_of beta.test)
echo "alpha gh-pages $A0, beta gh-pages $B0"

step "drop a post into alpha (written the way editors do: temp file, then rename)"
printf '# Hello from the folder\n\nWritten, saved, published.\n' > "$ROOT/alpha.test/posts/.hello.md.swp"
printf '# Hello from the folder\n\nWritten, saved, published.\n' > "$ROOT/alpha.test/posts/hello.md.tmp"
mv "$ROOT/alpha.test/posts/hello.md.tmp" "$ROOT/alpha.test/posts/hello.md"
rm "$ROOT/alpha.test/posts/.hello.md.swp"
wait_for 30 "alpha feed to contain the post" bash -c "git --git-dir='$T/alpha.test.git' show gh-pages:feed.xml | grep -q 'Hello from the folder'"
A1=$(head_of alpha.test)
URL=$(git --git-dir="$T/alpha.test.git" ls-tree -r --name-only gh-pages | grep '/hello/index.html')
echo "alpha now at $A1; post rendered at /$URL"
show alpha.test "$URL" | grep -o '<h1>Hello from the folder</h1>'
show alpha.test feed.xml | grep -o '<link>https://alpha.test/[^<]*hello/</link>'
show alpha.test feed.xml | grep -o 'Written, saved, published.'
[ "$(head_of beta.test)" = "$B0" ] && echo "beta untouched: still $B0"
if git --git-dir="$T/alpha.test.git" ls-tree -r --name-only gh-pages | grep -E '\.swp|\.tmp'; then echo "FAIL: editor debris published"; exit 1; fi
echo "no editor debris published"

step "break alpha's post template and add another post"
cp "$ROOT/alpha.test/theme/_layouts/post.html" "$T/post.html.bak"
printf '{%% extends "_layouts/base.html" %%}{%% block main %%}{%% if %%}{%% endblock %%}\n' > "$ROOT/alpha.test/theme/_layouts/post.html"
printf '# Second post\n\nShould not appear until fixed.\n' > "$ROOT/alpha.test/posts/second.md"
wait_for 30 "status to show the failure" bash -c "! '$BIN' status --root '$ROOT' >/dev/null"
set +e; "$BIN" status --root "$ROOT"; code=$?; set -e
echo "status exit code: $code"
[ "$(head_of alpha.test)" = "$A1" ] && echo "alpha deployed output unchanged: still $A1"
! show alpha.test feed.xml | grep -q 'Second post' && echo "second post not deployed"
[ -f "$ROOT/alpha.test/.blog/public/$URL" ] && echo "last good build still in .blog/public"

step "fix the template"
cp "$T/post.html.bak" "$ROOT/alpha.test/theme/_layouts/post.html"
wait_for 30 "second post to deploy" bash -c "git --git-dir='$T/alpha.test.git' show gh-pages:feed.xml | grep -q 'Second post'"
"$BIN" status --root "$ROOT"
[ "$(head_of beta.test)" = "$B0" ] && echo "beta still untouched: $B0"

step "watch log"
cat "$T/watch.log"
printf '\nE2E PASSED\n'
