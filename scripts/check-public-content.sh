#!/usr/bin/env bash
set -euo pipefail

script_dir=$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)

worktree_link=()
published_tree=0
if (($# == 0)); then
    source_root="$script_dir/.."
    worktree_link=(-path ./.git -type f -prune -o)
elif (($# == 2)) && [[ $1 == --source-tree ]]; then
    source_root=$2
    published_tree=1
else
    printf '%s\n' 'Usage: check-public-content.sh [--source-tree DIRECTORY]' >&2
    exit 2
fi

if [[ ! -d $source_root ]]; then
    printf '%s\n' 'Public content check failed: source tree is not a directory.' >&2
    exit 2
fi

repository_root=$(CDPATH= cd -- "$source_root" && pwd -P)

deny_short='i''gd'
deny_owner='edge''prosolutions'
deny_brand='ask''i''g''d'
deny_expression="(^|[^[:alnum:]])(${deny_short}|${deny_owner}|${deny_brand})([^[:alnum:]]|$)"
deny_second_project='ven''ted'
second_project_expression="(^|[^[:alnum:]])${deny_second_project}([^[:alnum:]]|$)"
second_project_url="https://${deny_second_project}.com"
quote="[\"'\`]"
second_project_attribution="s#${deny_second_project}: *${quote}${second_project_url//./\\.}${quote}##g; s#${second_project_url//./\\.}##g; s#footer__${deny_second_project}##g; s#site\\.${deny_second_project}##g; s#<b[^>]*>Ven""ted</b> project##g"
second_project_rendered="s#\\.${deny_second_project}([^[:alnum:]_]|\$)#\\1#g"
split_words='s#sha(1|256|384|512)-[A-Za-z0-9+/]+=*##g; s#base64,[A-Za-z0-9+/]+=*##g; s#([[:lower:][:digit:]])([[:upper:]])#\1 \2#g; s#([[:upper:]])([[:upper:]][[:lower:]])#\1 \2#g; s#_# #g'

# Additions require an owner-approved public release or an intentionally generic example.
readonly approved_public_names='mm api demo example notes store'
deny_working_brand='mod''mon'
working_brand_expression="(^|[^[:alnum:]_])${deny_working_brand}([^[:alnum:]_]|$)"
approved_site_domain='mod''mon''.dev'
approved_repository='github.com/''mod''mon''hq/''mod''mon'
lineage_word='[Tt][Aa][Ss][Kk]'
lineage_expression="([Mm][Mm][_-]?${lineage_word}[0-9]+|(^|[^[:alpha:]])${lineage_word}[-_ ][0-9]+|Task[0-9]+|task[0-9]{2,})"

home_root='/ho''me'
users_root='/Us''ers'
home_expression="(^|[^[:alnum:]_])(${home_root}|${users_root})/[^/[:space:]]+/"
windows_home_expression='[[:alpha:]]:\\Users\\[^\\[:space:]]+\\'
internal_host_expression='(^|[^[:alnum:]_])([[:alnum:]-]+\.)*(registry|docker|containers)\.(internal|corp|intranet|lan|local|private)([^[:alnum:]_]|$)'
private_mail_expression='@[[:alnum:]._-]+\.(internal|corp|intranet|lan|local|private)([^[:alnum:]_]|$)'
private_key_expression='-----BEGIN (OPENSSH|RSA|DSA|EC|PGP)? ?PRIVATE KEY-----'

has_forbidden_artifact_name() {
    local basename=${1##*/}
    local lowercase=${basename,,}

    case "$lowercase" in
        .env | .env.*)
            [[ "$lowercase" == *.example ]] || return 0
            ;;
        id_rsa | id_dsa | id_ecdsa | id_ed25519 | credentials.json | secrets.json | *.pem | *.p12 | *.pfx)
            return 0
            ;;
        *.key)
            [[ "$lowercase" == *.public.key ]] || return 0
            ;;
    esac
    return 1
}

violations=0
report_violation() {
    local path=$1
    local rule=$2
    printf 'Public content violation: %s [%s]\n' "$path" "$rule" >&2
    violations=$((violations + 1))
}

check_expression() {
    local subject=$1
    local path=$2
    local rule=$3
    local expression=$4

    if [[ $rule == unapproved-working-brand ]]; then
        # The local workspace path is allowed in guidance and tooling, and so are the
        # website's own hostname and the repository URL. This does not allow the
        # working brand as a product name or another hostname.
        if LC_ALL=C sed -E "s#${approved_repository//./\\.}(\\.git)?(\\.?)([^[:alnum:]_.-]|\$)#github.com/mm/mm\\1\\2\\3#g; s#(^|[^[:alnum:]_.-])${approved_site_domain//./\\.}([^[:alnum:]_.-]|\$)#\\1mm.dev\\2#g; s/\\.modmon([^[:alnum:]_.-]|$)/.workspace\\1/g" "$subject" | grep -I -Eiq -- "$expression"; then
            report_violation "$path" "$rule"
        fi
    elif [[ $rule == private-user-path ]]; then
        # Emscripten's virtual home directory is compiled into WebAssembly runtimes (the
        # site's in-browser SQLite); it names no real user.
        if LC_ALL=C sed "s#${home_root}/web_user\\([^[:alnum:]_.-]\\)#/emscripten-home\\1#g" "$subject" | grep -I -Eiq -- "$expression"; then
            report_violation "$path" "$rule"
        fi
    elif LC_ALL=C grep -I -Eiq -- "$expression" "$subject"; then
        report_violation "$path" "$rule"
    fi
}

check_words() {
    local subject=$1
    local path=$2
    local rule=$3
    local expression=$4
    local allowed=${5:-}

    if LC_ALL=C sed -E "${allowed:+$allowed; }$split_words" "$subject" | grep -I -Eiq -- "$expression"; then
        report_violation "$path" "$rule"
    fi
}

second_project_allowed() {
    case "$1" in
        site/app/app.config.ts | site/app/components/site/SiteFooter.vue) printf '%s' "$second_project_attribution" ;;
        site/.output/*) printf '%s; %s' "$second_project_attribution" "$second_project_rendered" ;;
        *) ((published_tree)) && printf '%s; %s' "$second_project_attribution" "$second_project_rendered" ;;
    esac
    return 0
}

check_case_sensitive_expression() {
    local subject=$1
    local path=$2
    local rule=$3
    local expression=$4

    if LC_ALL=C grep -I -Eq -- "$expression" "$subject"; then
        report_violation "$path" "$rule"
    fi
}

if (($# > 0)); then
    # Published trees must never contain the private workspace, even empty.
    if [[ -e "$repository_root/.modmon" || -L "$repository_root/.modmon" ]]; then
        report_violation '.modmon' 'private-workspace-in-artifact'
    fi
elif git -C "$repository_root" rev-parse --is-inside-work-tree >/dev/null 2>&1; then
    while IFS= read -r -d '' tracked; do
        report_violation "$tracked" 'tracked-private-workspace'
    done < <(git -C "$repository_root" ls-files -z -- .modmon)
fi

while IFS= read -r -d '' raw_path; do
    path=${raw_path#./}
    [[ $path == . ]] && continue
    lowercase_path=${path,,}
    basename=${lowercase_path##*/}

    case "/$lowercase_path/" in
        */tasks/*)
            report_violation "$path" 'agent-task-directory'
            ;;
    esac
    case "$basename" in
        .task*.json)
            report_violation "$path" 'agent-task-export'
            ;;
        coverage*.json | *-coverage.json | *.lcov | lcov.info | cobertura*.xml | clover*.xml | *.profraw | *.profdata)
            report_violation "$path" 'coverage-export'
            ;;
    esac

    path_subject="/$path"
    if LC_ALL=C sed -E "$split_words" <<<"$path_subject" | LC_ALL=C grep -Eiq -- "$deny_expression"; then
        report_violation "$path" 'private-client-identifier'
    fi
    if LC_ALL=C sed -E "$split_words" <<<"$path_subject" | LC_ALL=C grep -Eiq -- "$second_project_expression"; then
        report_violation "$path" 'second-project-name'
    fi
    if LC_ALL=C grep -Eiq -- "$working_brand_expression" <<<"$path_subject"; then
        report_violation "$path" 'unapproved-working-brand'
    fi
    if LC_ALL=C grep -Eq -- "$lineage_expression" <<<"$path_subject"; then
        report_violation "$path" 'agent-task-lineage'
    fi
    if has_forbidden_artifact_name "$path"; then
        report_violation "$path" 'credential-like-filename'
    fi
    if LC_ALL=C grep -Eiq -- "$home_expression|$windows_home_expression" <<<"$path_subject"; then
        report_violation "$path" 'private-user-path'
    fi
    if LC_ALL=C grep -Eiq -- "$internal_host_expression" <<<"$path_subject"; then
        report_violation "$path" 'private-hostname'
    fi
    if LC_ALL=C grep -Eiq -- "$private_mail_expression" <<<"$path_subject"; then
        report_violation "$path" 'private-email'
    fi

    file="$repository_root/$path"
    [[ -f "$file" ]] || continue
    # Nuxt Content's search dump is gzip in base64: its text is random, so a
    # pattern matches by chance. The same documents are checked as Markdown
    # and as the rendered pages beside it.
    [[ $basename == sql_dump.txt && $lowercase_path == *__nuxt_content/* ]] && continue
    check_words "$file" "$path" 'private-client-identifier' "$deny_expression"
    check_words "$file" "$path" 'second-project-name' "$second_project_expression" "$(second_project_allowed "$path")"
    check_expression "$file" "$path" 'unapproved-working-brand' "$working_brand_expression"
    check_case_sensitive_expression "$file" "$path" 'agent-task-lineage' "$lineage_expression"
    check_expression "$file" "$path" 'private-user-path' "$home_expression|$windows_home_expression"
    check_expression "$file" "$path" 'private-hostname' "$internal_host_expression"
    check_expression "$file" "$path" 'private-email' "$private_mail_expression"
    check_expression "$file" "$path" 'private-key-material' "$private_key_expression"
done < <(
    cd "$repository_root"
    find . \
        "${worktree_link[@]}" \
        -path './.modmon' -prune -o \
        -path './site/.data' -prune -o \
        -type d \( \
            -name .git -o \
            -name target -o \
            -name node_modules -o \
            -name .nuxt -o \
            -name vendor -o \
            -name coverage -o \
            -name playwright-report -o \
            -name test-results \
        \) -prune -o -print0
)

if ((violations > 0)); then
    printf '%s\n' 'Public content check failed: prohibited public-release material detected.' >&2
    exit 1
fi

printf '%s\n' 'Public content check passed.'
