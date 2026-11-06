//! Drafts a mod index entry from a KeenWiki page: `pnpm index:add <link>`.

fn main() -> std::process::ExitCode {
    bloogbase_lib::index_add(std::env::args().skip(1).collect())
}
