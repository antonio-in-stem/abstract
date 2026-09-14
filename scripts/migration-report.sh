#!/usr/bin/env bash
#
# Compiles every project under a corpus directory with the Abstract 1.0
# binary and writes a migration report: the diagnostics each project
# reports, and the edit each one implies for a 0.2.0 source becoming 1.0.
#
# The corpus is READ ONLY. This script never writes, moves or deletes anything
# under it: it runs `abstract compile <project> JSON` with stdout discarded, so
# the compiler only reads the discovered sources and the assets they name
# (SPEC 7.6), and every artefact it produces goes to the report path below.
#
# Usage:
#   scripts/migration-report.sh <corpus-directory>
#
# Environment:
#   ABSTRACT_CORPUS           corpus directory (overridden by the argument)
#   ABSTRACT_BIN              compiler binary (default: target/release, then
#                             target/debug, then `cargo run --release`)
#   ABSTRACT_REPORT           report path (default: <repo>/migration-report.md)
#
# Exit status: 0 when the report was written, 1 when it could not be.
# A corpus that fails to compile is the expected outcome and is not an error
# of this script.

set -u

script_dir=$(cd -- "$(dirname -- "$0")" && pwd)
project_dir=$(cd -- "$script_dir/.." && pwd)
repo_dir=$project_dir

corpus=${1:-${ABSTRACT_CORPUS:-}}
report=${ABSTRACT_REPORT:-$repo_dir/migration-report.md}

if [ -z "$corpus" ] || [ ! -d "$corpus" ]; then
    echo "migration-report: corpus not given or not found: ${corpus:-<none>}" >&2
    echo "migration-report: pass it as an argument or set ABSTRACT_CORPUS." >&2
    exit 1
fi

# ---------------------------------------------------------------- the binary

binary=${ABSTRACT_BIN:-}
if [ -z "$binary" ]; then
    for candidate in \
        "$project_dir/target/release/abstract.exe" \
        "$project_dir/target/release/abstract" \
        "$project_dir/target/debug/abstract.exe" \
        "$project_dir/target/debug/abstract"; do
        if [ -x "$candidate" ]; then
            binary=$candidate
            break
        fi
    done
fi
if [ -z "$binary" ]; then
    echo "migration-report: building the compiler" >&2
    (cd "$project_dir" && cargo build --release) || exit 1
    for candidate in \
        "$project_dir/target/release/abstract.exe" \
        "$project_dir/target/release/abstract"; do
        if [ -x "$candidate" ]; then
            binary=$candidate
            break
        fi
    done
fi
if [ -z "$binary" ] || [ ! -x "$binary" ]; then
    echo "migration-report: no abstract binary; set ABSTRACT_BIN." >&2
    exit 1
fi

version=$("$binary" --version 2>/dev/null | tr -d '\r')

# --------------------------------------------------------------- the projects
#
# A project root is a top-level entry of the corpus that holds at least one
# `.ab` or `.abt` file somewhere below it. `abstract` then applies SPEC 2.3
# itself: a root with a `data/` directory discovers that directory and treats
# the root as the project root, and one without discovers the root itself.
# `target/` and dot-directories are build output, never sources (SPEC 2.4).

projects=()
while IFS= read -r entry; do
    name=$(basename -- "$entry")
    case "$name" in
        .*|target|node_modules|build|out) continue ;;
    esac
    if find "$entry" \( -name '*.ab' -o -name '*.abt' \) -print -quit 2>/dev/null | grep -q .; then
        projects+=("$entry")
    fi
done < <(find "$corpus" -mindepth 1 -maxdepth 1 -type d | sort)

if [ ${#projects[@]} -eq 0 ]; then
    if find "$corpus" -maxdepth 1 \( -name '*.ab' -o -name '*.abt' \) -print -quit 2>/dev/null | grep -q .; then
        projects+=("$corpus")
    fi
fi

if [ ${#projects[@]} -eq 0 ]; then
    echo "migration-report: no .ab or .abt sources under $corpus" >&2
    exit 1
fi

# ------------------------------------------------------- the migration table
#
# What each diagnostic identifier means for a 0.2.0 source that must become a
# 1.0 one. The identifier is the compiler's; the edit is what the author has
# to write instead, with the section that fixes it.

migration_for() {
    case "$1" in
    E101) echo "Make the file readable, or drop it: discovery collected it and every discovered source must be read (SPEC 2.4)." ;;
    E102) echo "Re-save the file as UTF-8; SPEC 2.2 admits no other encoding, and a BOM is allowed only at offset 0." ;;
    E103) echo "Point the command at the project root or its 'data' directory; the named directory holds no sources (SPEC 2.3)." ;;
    E201) echo "Close the string on its own line; a quoted value never spans lines (SPEC 3.5)." ;;
    E202) echo "Use one of the five escapes the language defines: \\\" \\\\ \\n \\r \\t (SPEC 3.5). A Windows path is written with '/'." ;;
    E203) echo "Close the bracket. A '{' or '(' left open inside a value swallows the rest of the file, because a line terminator does not end a logical line while the value-bracket stack is non-empty (SPEC 3.6)." ;;
    E204) echo "Close each bracket with its own kind (SPEC 3.6)." ;;
    E205) echo "Delete the stray closing bracket, or open the one it was meant to close (SPEC 3.6)." ;;
    E206) echo "Rewrite the identifier with A-Z a-z 0-9 _ - only; non-ASCII letters are not identifier characters (SPEC 3.3). Keep the accented text as a quoted VALUE instead of a field name." ;;
    E207) echo "Remove the trailing '-' from the identifier (SPEC 3.3)." ;;
    E208) echo "A schema name starts with a letter and holds letters, digits and '_': no spaces (SPEC 3.4)." ;;
    E209) echo "Reduce the nesting or the path length below the limit of SPEC 3.7." ;;
    E210) echo "The construct is not part of the 1.0 grammar here. Most often this is 0.2.0 syntax Appendix A removed — a loop variable is now always written '\$name' (A44) — or illustrative output pasted into a source file, which belongs in a '//' comment (SPEC 3.2)." ;;
    E211) echo "Write a number the language can hold: int is a 64-bit integer, float a finite double (SPEC 3.5, 4.4.2, 4.4.3)." ;;
    E301) echo "Give the schema a unique name, or delete the second declaration; schema names are project-wide (SPEC 4.2)." ;;
    E302) echo "Rename one of the two fields; names are compared after normalisation, so 'Name' and 'name' collide (SPEC 3.3, 4.3)." ;;
    E303) echo "Use a modifier the language defines: @optional, @tag, @since(n), @removed(n) (SPEC 4.3, 4.12)." ;;
    E304) echo "Name one of the nine types and glue the '(' to it: 'text(1..40)', never 'text (1..40)' (SPEC 4.4)." ;;
    E305) echo "Write the range as 'value' or 'min..max' with min <= max and both finite (SPEC 4.4.2)." ;;
    E306) echo "Give the enum at least one member and no repeats (SPEC 4.4.5)." ;;
    E307) echo "Use one of png, jpg, gif, bmp, webp (SPEC 4.4.7)." ;;
    E308) echo "Write the size as WIDTHxHEIGHT, each side a number or '*' (SPEC 4.4.7)." ;;
    E309) echo "Declare the schema the reference names, or fix the spelling (SPEC 4.4.8, 4.4.9)." ;;
    E310) echo "One group carries at most one @tag field (SPEC 4.8)." ;;
    E311) echo "@tag marks the key of a LIST OF GROUPS, so it belongs to a field inside a group, never to a root field. Move the field into the group it keys, or drop @tag (SPEC 4.8, Appendix A). This is the commonest 0.2.0 schema to repair." ;;
    E312) echo "A group field carries no default; give the fields inside it defaults instead (SPEC 4.7)." ;;
    E313) echo "Make the default satisfy its own field's type, range and enum: a default is validated exactly as an authored value (SPEC 4.10)." ;;
    E314) echo "'template' and 'id' are written by the compiler. Rename the field, and declare an id only as 'id: text' or 'id: text(min..max)' (SPEC 4.11)." ;;
    E315) echo "Write '[]' once; a field is a list or it is not (SPEC 4.6)." ;;
    E316) echo "Every path segment is an identifier: no empty segment ('a..b', '.a', 'a.'), no index ('name[0]'), and in a schema no punctuation where the field name belongs (SPEC 4.3, 5.4)." ;;
    E317) echo "Give the parameterised type at least one argument: 'file(png)', not 'file()' (SPEC 4.4)." ;;
    E318) echo "The constraint can never be satisfied; widen it (SPEC 4.4.2, 4.4.7)." ;;
    E319) echo "Declare each extension once (SPEC 4.4.6)." ;;
    E320) echo "Choose one: a default already makes the field present, so @optional adds nothing (SPEC 4.10)." ;;
    E321) echo "Break the cycle with @optional or a list on one edge; a chain of required, non-list \$(Schema) fields can never terminate (SPEC 4.4.9)." ;;
    E322) echo "@tag needs a non-list field of type text, int, bool or enum (SPEC 4.8)." ;;
    E323) echo "Write '[]', '[min..]' or '[min..max]' with 0 <= min <= max (SPEC 4.6)." ;;
    E401) echo "Declare the schema the header names, or fix the spelling (SPEC 5.1)." ;;
    E402) echo "Ids are unique project-wide. Rename the instance or its file: an instance with no '@id' takes its id from the file stem (SPEC 5.3)." ;;
    E403) echo "Every statement belongs to an instance, so the instance header comes first. A clone written above the header moves below it, where SPEC 5.7 requires it: 'Sticker :: @id.x' then '&source.*'. A header wrapped onto a second line also orphans what follows unless the first line ends with ',' (SPEC 3.6, 5.1)." ;;
    E404) echo "Write every clone immediately after the header, before any assignment (SPEC 5.7)." ;;
    E405) echo "Declare the instance the clone names, or fix the spelling (SPEC 5.7)." ;;
    E406) echo "Break the clone cycle (SPEC 5.7)." ;;
    E407) echo "A clone source uses the same schema as the cloning instance (SPEC 5.7)." ;;
    E408) echo "Assign the path on the clone source, or clone a path it actually has (SPEC 5.7)." ;;
    E409) echo "Declare the field in the schema, or delete the statement: 1.0 has no lenient mode and no '--allow-unknown' (SPEC 5.12, Appendix A52)." ;;
    E410) echo "'template' and 'id' are written by the compiler; use '@id.<name>' in the header to choose the id and delete the assignment (SPEC 5.3, 5.4)." ;;
    E411) echo "Give the field a value, or declare it @optional or with a default (SPEC 4.10, 5.12)." ;;
    E412) echo "Write the value in the shape the declared type takes; quoting a number or a boolean makes it text (SPEC 5.10)." ;;
    E413) echo "Bring the value inside the declared range; an id is also range-checked, against 'id: text(1..64)' when the schema declares none (SPEC 4.11)." ;;
    E414) echo "Use a declared enum member, or add the member to the schema (SPEC 4.4.5)." ;;
    E415) echo "The prefix wildcard matches no member; fix the prefix or the enum (SPEC 5.6)." ;;
    E416) echo "Give the tuple row exactly as many values as the column list declares (SPEC 5.5)." ;;
    E417) echo "Name each tuple column once (SPEC 5.5)." ;;
    E418) echo "'#tag' shorthand needs a list of groups with a @tag field; add the @tag, or write the elements out (SPEC 5.5, 4.8)." ;;
    E419) echo "A '#tag' argument is 'key: value' or a bare boolean flag (SPEC 5.5)." ;;
    E420) echo "Use an extension the field declares, or declare the one the asset uses (SPEC 4.4.6, 4.4.7)." ;;
    E421) echo "Put the asset under '<project root>/assets' at the path the value names, or fix the value; asset paths are relative to assets/ (SPEC 5.9)." ;;
    E422) echo "The file's header does not match its extension; re-export it, or rename it to what it is (SPEC 4.4.7.1)." ;;
    E423) echo "Resize the image, or widen the declared size; '*' accepts any extent on that side (SPEC 4.4.7)." ;;
    E424) echo "Asset paths stay inside assets/: no leading '/', no drive letter, no '..' (SPEC 5.9)." ;;
    E425) echo "Interpolation reads the root scalar fields of the same instance; declare the field or fix the name (SPEC 5.11)." ;;
    E426) echo "Write '\$name', '\${name}' or '\$\$' for a literal '\$' (SPEC 5.11)." ;;
    E427) echo "Close the '\${' (SPEC 5.11)." ;;
    E428) echo "An instance id is an identifier: '@id.<name>' with a value, never a bare '@id' flag and never a name with spaces or punctuation (SPEC 5.2, 5.3)." ;;
    E429) echo "One path is written once per version. Merge the two statements, or scope them with @since/@removed so their version sets are disjoint (SPEC 5.4, 5.13)." ;;
    E430) echo "The statement applies to no version; correct the annotation or the field's own @since/@removed (SPEC 5.13)." ;;
    E431) echo "The 'ref' target does not exist in that version; declare it, or widen its window (SPEC 4.4.8, 5.14)." ;;
    E432) echo "The 'ref' target uses another schema (SPEC 4.4.8)." ;;
    E433) echo "A multi-path names at least one key: 'a.{b, c}: v' (SPEC 5.4)." ;;
    E434) echo "A brace pattern expands into several paths, so it needs a list field (SPEC 5.8)." ;;
    E435) echo "Each brace group holds non-empty alternatives separated by commas and does not nest (SPEC 5.8)." ;;
    E436) echo "An instance header holds exactly one '::' (SPEC 5.1)." ;;
    E437) echo "@since/@removed go on a body statement or at the end of the header, never on a tag, a clone or a block (SPEC 5.13)." ;;
    E438) echo "A header tag assigns one ROOT SCALAR field; write a body statement for anything nested (SPEC 5.2)." ;;
    E439) echo "The text before '::' must be a schema name (SPEC 5.1)." ;;
    E440) echo "The statement or clone reaches outside the instance's own version window; widen the window or narrow the annotation (SPEC 5.14)." ;;
    E441) echo "Lists do not nest and no item is empty (SPEC 5.5)." ;;
    E442) echo "A trailing ',' does not continue a statement onto the next line. Put the whole value on one logical line, or wrap it in brackets, which do continue it (SPEC 3.6)." ;;
    E443) echo "The path crosses a value that is not an object, or a list. Write list elements with a tuple array or with '#tag' shorthand (SPEC 5.4)." ;;
    E444) echo "A keyed list holds at most one element per tag value; note that a wildcard expands before the check (SPEC 5.5, 5.6)." ;;
    E445) echo "The list has the wrong number of elements for its declared cardinality (SPEC 4.6)." ;;
    E501) echo "A logic block names a declared schema (SPEC 6.1)." ;;
    E502) echo "One schema has one logic block; merge them (SPEC 6.1)." ;;
    E503) echo "Every literal path segment in logic names a declared field (SPEC 6.5)." ;;
    E504) echo "'derive' cannot write 'template' or 'id' (SPEC 6.4)." ;;
    E505) echo "A 'derive' target is a declared field of the block's own schema (SPEC 6.4)." ;;
    E506) echo "'derive' cannot write through a list (SPEC 6.4)." ;;
    E507) echo "Every 'require' carries 'else throw \"message\"' (SPEC 6.2)." ;;
    E508) echo "A throw message is a quoted string (SPEC 6.2)." ;;
    E509) echo "'for' iterates a list field or a literal list (SPEC 6.2)." ;;
    E510) echo "The loop variable is not bound here; loop variables are always written '\$name' (SPEC 6.9, Appendix A44)." ;;
    E511) echo "The condition is not well formed; comparisons are '==', '!=', '<', '<=', '>', '>=', 'contains' and 'exists' (SPEC 6.7)." ;;
    E512) echo "Comparisons do not chain; join them with 'and' (SPEC 6.7)." ;;
    E513) echo "The operator cannot compare those two types (SPEC 6.7)." ;;
    E514) echo "'length()' takes a list or a text value (SPEC 6.10)." ;;
    E515) echo "A 'require' in the project's own logic failed: the data does not satisfy the rule the schema states (SPEC 6.2)." ;;
    E516) echo "Rename the inner loop variable; an enclosing loop already binds it (SPEC 6.2)." ;;
    E517) echo "'else' follows the closing '}' of its 'if' block (SPEC 6.3)." ;;
    E518) echo "'derive?' writes only a field with no default, because defaults are filled before logic runs (SPEC 6.4, 7.3)." ;;
    E519) echo "The operand projects several values because the path crosses a list; use 'contains' (SPEC 6.5)." ;;
    E520) echo "A 'derive' target names fields, never a loop variable (SPEC 6.4)." ;;
    E521) echo "Guard the 'derive' with a version condition: the field does not exist in every version (SPEC 6.6, 4.12)." ;;
    E601) echo "One project declares one 'versions' range (SPEC 4.12)." ;;
    E602) echo "Write 'versions min..max' with integers >= 1 and min <= max (SPEC 4.12)." ;;
    E603) echo "The annotation names a version outside the project range; widen 'versions' or change the annotation (SPEC 4.12)." ;;
    E604) echo "'@removed(r)' must be greater than '@since(s)' (SPEC 4.12)." ;;
    E605) echo "The field exists in no version; the annotations cancel out against the project range or the enclosing group (SPEC 4.12)." ;;
    E701) echo "The value cannot be rendered in this format (SPEC 8.7)." ;;
    E8*) echo "A command-line defect, not a source defect: see SPEC 9." ;;
    *) echo "See the entry for this identifier in SPEC chapter 10." ;;
    esac
}

# ------------------------------------------------------------------ the run

tmp_dir=$(mktemp -d 2>/dev/null || mktemp -d -t abstract-private)
if [ -z "$tmp_dir" ] || [ ! -d "$tmp_dir" ]; then
    echo "migration-report: cannot create a temporary directory" >&2
    exit 1
fi
trap 'rm -rf "$tmp_dir"' EXIT

report_dir=$(dirname -- "$report")
mkdir -p "$report_dir" || exit 1

{
    echo "# Migration report"
    echo
    echo "Every project under the named corpus, compiled with the Abstract 1.0"
    echo "compiler, and the migration edit each diagnostic implies."
    echo
    echo "- Corpus: \`$corpus\` (read only: this run wrote nothing under it)"
    echo "- Compiler: \`${version:-unknown}\`"
    echo "- Command per project: \`abstract compile <project> JSON --max-errors 0\`, stdout discarded"
    echo
    echo "The corpus is only read: this run writes nothing under it, and the"
    echo "report quotes only the diagnostics the compiler printed."
    echo
} > "$report"

total_projects=0
clean_projects=0
total_diagnostics=0
: > "$tmp_dir/all-ids"

for project in "${projects[@]}"; do
    total_projects=$((total_projects + 1))
    name=${project#"$corpus"/}
    [ "$name" = "$project" ] && name=$(basename -- "$project")

    "$binary" compile "$project" JSON --max-errors 0 \
        > /dev/null 2> "$tmp_dir/stderr"
    status=$?
    tr -d '\r' < "$tmp_dir/stderr" > "$tmp_dir/clean"

    # A diagnostic line is `path:line:col: error[Exxx]: message`; a `note:`
    # line belongs to the diagnostic above it (SPEC 9.8).
    grep -E 'error\[E[0-9]+\]:' "$tmp_dir/clean" > "$tmp_dir/lines" 2>/dev/null
    count=$(wc -l < "$tmp_dir/lines" | tr -d ' ')
    total_diagnostics=$((total_diagnostics + count))

    {
        echo "## \`$name\`"
        echo
        echo "Exit code $status, $count diagnostic(s)."
        echo
    } >> "$report"

    if [ "$count" -eq 0 ]; then
        clean_projects=$((clean_projects + 1))
        {
            if [ "$status" -eq 0 ]; then
                echo "Compiles cleanly. No migration edit is needed."
            else
                echo "The compiler failed without printing a diagnostic line; raw stderr:"
                echo
                echo '```'
                cat "$tmp_dir/clean"
                echo '```'
            fi
            echo
        } >> "$report"
        continue
    fi

    {
        echo "| Where | ID | Diagnostic |"
        echo "|---|---|---|"
    } >> "$report"
    while IFS= read -r line; do
        id=$(printf '%s' "$line" | sed -n 's/.*error\[\(E[0-9]*\)\].*/\1/p')
        where=$(printf '%s' "$line" | sed -n 's/^\(.*\): error\[E[0-9]*\]:.*/\1/p')
        message=$(printf '%s' "$line" | sed -n 's/.*error\[E[0-9]*\]: //p')
        [ -z "$where" ] && where='(no position)'
        # `|` would break the table; the messages quote no pipes today.
        message=$(printf '%s' "$message" | sed 's/|/\\|/g')
        echo "| \`$where\` | $id | $message |" >> "$report"
        echo "$id" >> "$tmp_dir/all-ids"
    done < "$tmp_dir/lines"
    echo >> "$report"

    {
        echo "### Migration edits"
        echo
    } >> "$report"
    sed -n 's/.*error\[\(E[0-9]*\)\].*/\1/p' "$tmp_dir/lines" | sort | uniq -c \
        | while read -r occurrences id; do
            echo "- **$id** ($occurrences occurrence(s)) — $(migration_for "$id")" >> "$report"
        done
    echo >> "$report"
done

{
    echo "## Summary"
    echo
    echo "- Projects compiled: $total_projects"
    echo "- Projects that compile cleanly: $clean_projects"
    echo "- Diagnostics reported: $total_diagnostics"
    echo
} >> "$report"

if [ -s "$tmp_dir/all-ids" ]; then
    {
        echo "Diagnostics by identifier, most frequent first:"
        echo
        echo "| ID | Count |"
        echo "|---|---|"
    } >> "$report"
    sort "$tmp_dir/all-ids" | uniq -c | sort -k1,1nr -k2,2 \
        | while read -r occurrences id; do
            echo "| $id | $occurrences |" >> "$report"
        done
    echo >> "$report"
fi

echo "migration-report: $total_diagnostics diagnostic(s) over $total_projects project(s)"
echo "migration-report: report written to $report"
exit 0
