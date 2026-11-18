#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)
scanner="$script_dir/check-public-content.sh"
temporary=$(mktemp -d)
trap 'rm -rf "$temporary"' EXIT
case_number=0

new_case() {
    case_number=$((case_number + 1))
    case_root="$temporary/case-$case_number"
    mkdir -p "$case_root/scripts"
    cp "$scanner" "$case_root/scripts/check-public-content.sh"
    chmod +x "$case_root/scripts/check-public-content.sh"
    git -C "$case_root" init -q
}

new_non_git_case() {
    new_case
    rm -rf "$case_root/.git"
}

add_file() {
    local path=$1
    local content=$2
    mkdir -p "$(dirname -- "$case_root/$path")"
    printf '%s\n' "$content" >"$case_root/$path"
    if [[ -d $case_root/.git ]]; then
        git -C "$case_root" add "$path" scripts/check-public-content.sh
    fi
}

expect_pass() {
    if ! "$case_root/scripts/check-public-content.sh" >/dev/null; then
        printf 'Expected public-content case %s to pass.\n' "$case_number" >&2
        exit 1
    fi
}

expect_failure() {
    if "$case_root/scripts/check-public-content.sh" >/dev/null 2>&1; then
        printf 'Expected public-content case %s to fail.\n' "$case_number" >&2
        exit 1
    fi
}

expect_failure_without_content() {
    local prohibited=$1
    local output
    if output=$("$case_root/scripts/check-public-content.sh" 2>&1); then
        printf 'Expected public-content case %s to fail.\n' "$case_number" >&2
        exit 1
    fi
    if [[ $output == *"$prohibited"* ]]; then
        printf 'Public-content case %s exposed prohibited content.\n' "$case_number" >&2
        exit 1
    fi
    if [[ $output != *'README.md [private-client-identifier]'* ]]; then
        printf 'Public-content case %s omitted its path or rule identifier.\n' "$case_number" >&2
        exit 1
    fi
}

blocked='i''gd'
blocked_owner='edge''prosolutions'
blocked_brand='ask''i''g''d'
blocked_working_brand='mod''mon'
blocked_mm_lineage='MM_''TASK''15'
blocked_resource_lineage='mm-''task''06'
blocked_comment_lineage='Task''-22'
blocked_bare_lineage='task''15'
blocked_second='ven''ted'

new_case
add_file "notes/${blocked}-report.txt" 'neutral text'
expect_failure

new_case
add_file 'README.md' "contains ${blocked} reference"
expect_failure

new_case
add_file 'README.md' "contains ${blocked^^} reference"
expect_failure

new_case
add_file 'README.md' "contains ${blocked_owner^^} reference"
expect_failure

new_case
add_file 'README.md' "contains ${blocked_brand} reference"
expect_failure

new_case
add_file 'README.md' 'bigdata is a neutral word with no identifier boundary'
expect_pass

new_case
add_file 'README.md' "contains ${blocked_working_brand} working brand"
expect_failure

new_case
add_file '.gitignore' '/.modmon/*'
add_file 'AGENTS.md' 'Keep local plans in `.modmon/` and never commit them.'
mkdir -p "$case_root/.modmon"
printf '%s\n' "contains ${blocked} working notes" >"$case_root/.modmon/notes.md"
expect_pass
git -C "$case_root" add -f .modmon/notes.md
expect_failure

new_non_git_case
mkdir -p "$case_root/.modmon"
if "$scanner" --source-tree "$case_root" >/dev/null 2>&1; then
    printf '%s\n' 'Expected private workspace in published tree to fail.' >&2
    exit 1
fi

new_case
add_file 'README.md' "An unrelated .${blocked_working_brand}-brand is not the workspace path."
expect_failure

new_case
add_file 'README.md' "https://get.${blocked_working_brand}.org/install.sh"
expect_failure

new_case
add_file 'README.md' "curl -LsSf https://${blocked_working_brand}.dev/install.sh | sh"
expect_pass

new_case
add_file 'README.md' "https://downloads.${blocked_working_brand}.org/install.sh"
expect_failure

new_case
add_file 'site/app/app.config.ts' "url: \"https://${blocked_working_brand}.dev\","
expect_pass

new_case
add_file 'README.md' "https://docs.${blocked_working_brand}.dev/"
expect_failure

new_case
add_file 'README.md' "https://${blocked_working_brand}.dev.example/"
expect_failure

for repository in \
    "https://github.com/${blocked_working_brand}hq/${blocked_working_brand}" \
    "https://github.com/${blocked_working_brand}hq/${blocked_working_brand}.git" \
    "https://github.com/${blocked_working_brand}hq/${blocked_working_brand}/raw/v0.1.1/packaging/release/allowed_signers" \
    "tags to github.com/${blocked_working_brand}hq/${blocked_working_brand}. The repository"; do
    new_case
    add_file 'README.md' "Clone $repository"
    expect_pass
done

for repository in \
    "https://github.com/${blocked_working_brand}hq/${blocked_working_brand}-fork" \
    "https://github.com/${blocked_working_brand}hq/${blocked_working_brand}.evil" \
    "https://github.com/someone/${blocked_working_brand}" \
    "${blocked_working_brand}hq/${blocked_working_brand}"; do
    new_case
    add_file 'README.md' "Clone $repository"
    expect_failure
done

new_non_git_case
add_file 'README.md' 'neutral public text'
printf 'gitdir: %s/someone/%s/.git/worktrees/release\n' '/ho''me' "$blocked_working_brand" >"$case_root/.git"
expect_pass
if "$scanner" --source-tree "$case_root" >/dev/null 2>&1; then
    printf '%s\n' 'Expected a .git file in a published tree to fail.' >&2
    exit 1
fi

for content in "${blocked^^} client" "ask${blocked^}" "${blocked^}Client" "${blocked^^}Client" "${blocked}_x" "x_${blocked}" "${blocked_brand^^}" "${blocked_owner^}" "${blocked_second}" "${blocked_second^}Client" "use_${blocked_second}"; do
    new_case
    add_file 'README.md' "contains $content"
    expect_failure
done

for content in 'InitializedConfigDocument' 'bigdata and bigdecimal' "if (event.defaultPre${blocked_second}) return;" "data:font/woff2;base64,AA${blocked^}Q+/x${blocked_second^}=="; do
    new_case
    add_file 'README.md' "$content"
    expect_pass
done

new_case
add_file 'site/package-lock.json' "\"integrity\": \"sha512-Ab${blocked^}Xy+/0${blocked_second}Q==\""
expect_pass

new_case
add_file 'site/app/app.config.ts' "    ${blocked_second}: \"https://${blocked_second}.com\","
add_file 'site/app/components/site/SiteFooter.vue' "<a :href=\"site.${blocked_second}\" class=\"footer__${blocked_second}\"><span>A <b>Ven""ted</b> project</span></a>
.footer__${blocked_second} { color: red; }"
expect_pass

new_case
add_file 'site/app/app.config.ts' "    ${blocked_second}Api: \"https://api.example.com\","
expect_failure

new_case
add_file 'README.md' "<a href=\"https://${blocked_second}.com\" class=\"footer__${blocked_second}\"><span>A <b>Ven""ted</b> project</span></a>"
expect_failure

new_case
add_file 'site/.output/public/index.html' "<a href=\"https://${blocked_second}.com\" class=\"footer__${blocked_second}\"><span data-v-1>A <b data-v-1>Ven""ted</b> project</span></a>"
expect_pass
add_file 'site/.output/public/docs/index.html' "Built for the Ven""ted platform"
expect_failure

new_non_git_case
add_file 'index.html' "<a href=\"https://${blocked_second}.com\" class=\"footer__${blocked_second}\"><span data-v-1>A <b data-v-1>Ven""ted</b> project</span></a>
<style>.footer__${blocked_second}[data-v-1]{color:red}</style>"
add_file '_nuxt/app.js' "const a={${blocked_second}:\`https://${blocked_second}.com\`,b:\"https://${blocked_second}.com\"};h(\"a\",{href:o(e).${blocked_second}})"
if ! "$scanner" --source-tree "$case_root" >/dev/null; then
    printf '%s\n' 'Expected the rendered footer attribution to pass in a published tree.' >&2
    exit 1
fi
add_file 'index.html' "Built for the Ven""ted platform"
if "$scanner" --source-tree "$case_root" >/dev/null 2>&1; then
    printf '%s\n' 'Expected the second project name outside the attribution to fail in a published tree.' >&2
    exit 1
fi

new_case
add_file 'site/package-lock.json' '"resolved": "https://registry.npmjs.org/queue-microtask/-/queue-microtask-1.2.3.tgz"'
expect_pass

new_case
add_file 'site/.nuxt/dev/index.mjs' "const root = \"${blocked_working_brand}\";"
expect_pass

new_case
add_file 'site/.output/public/_nuxt/sqlite.js' 'FS.mkdir("/ho''me/web_user/");'
expect_pass

new_case
add_file 'site/.output/public/_nuxt/app.js' 'const root = "/ho''me/web_user_real/project/";'
expect_failure

new_case
add_file 'core/tests/secret.rs' "const VALUE: &str = \"${blocked_mm_lineage}_STATUS_SECRET\";"
expect_failure

new_case
add_file "fixtures/${blocked_resource_lineage}-network.yaml" 'name: runtime network'
expect_failure

new_case
add_file 'site/.output/public/docs/reference/cli/index.html' "<!-- ${blocked_comment_lineage}-authorized -->"
expect_failure

new_case
add_file 'scripts/live-secret.sh' "project=${blocked_bare_lineage}"
expect_failure

new_case
add_file 'README.md' 'Scheduler task objects have task states; fixture record task1 is generic.'
expect_pass

new_case
add_file 'examples/README.md' 'Approved semantic examples: mm, api, demo, example, notes, and store.'
expect_pass

new_case
for directory in target node_modules vendor coverage playwright-report test-results; do
    add_file "$directory/${blocked}-fixture.txt" "contains ${blocked} reference"
done
expect_pass

new_case
add_file '.gitignore' 'ignored.txt'
printf '%s\n' "contains ${blocked} reference" >"$case_root/ignored.txt"
expect_failure

new_non_git_case
add_file 'README.md' 'neutral public text'
expect_pass

new_case
add_file 'tasks/brief.md' 'neutral text'
if "$scanner" --source-tree "$case_root" >/dev/null 2>&1; then
    printf 'Expected source-tree case %s to fail.\n' "$case_number" >&2
    exit 1
fi

new_non_git_case
add_file '.task-current.json' '{}'
if "$scanner" --source-tree "$case_root" >/dev/null 2>&1; then
    printf 'Expected source-tree case %s to fail.\n' "$case_number" >&2
    exit 1
fi

new_non_git_case
add_file 'coverage-final.json' '{}'
if "$scanner" --source-tree "$case_root" >/dev/null 2>&1; then
    printf 'Expected source-tree case %s to fail.\n' "$case_number" >&2
    exit 1
fi

new_non_git_case
add_file 'README.md' 'neutral public text'
if ! "$scanner" --source-tree "$case_root" >/dev/null; then
    printf 'Expected source-tree case %s to pass.\n' "$case_number" >&2
    exit 1
fi

new_case
add_file 'README.md' "/"'home/developer/private-project'
expect_failure

new_case
add_file 'README.md' 'C:'"\\Users\\developer\\private-project"
expect_failure

new_case
add_file 'README.md' 'registry.'"internal/acme/web"
expect_failure

new_case
add_file 'tests/proxy.mjs' 'const upstream = "host.docker.'"internal"'";'
expect_failure

new_case
add_file 'README.md' 'ops@acme.'"internal"
expect_failure

new_case
add_file '.env.production' 'TOKEN=materialized-value'
expect_failure

new_case
add_file 'deploy.key' 'materialized-value'
expect_failure

new_case
add_file 'README.md' '-----BEGIN '"PRIVATE KEY-----"
expect_failure

new_case
secret_content="sensitive-${blocked}-value"
add_file 'README.md' "$secret_content"
expect_failure_without_content "$secret_content"

printf '%s\n' 'Public content scanner tests passed.'
