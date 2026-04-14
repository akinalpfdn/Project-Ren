//! File and folder tools — open a well-known user folder, list its contents,
//! or read a text file from it. All paths flow through the shared `paths`
//! sandbox so the LLM cannot wander outside the allow-listed roots.

pub mod folders;
pub mod list_dir;
pub mod paths;
pub mod read_text;

pub use folders::OpenFolder;
pub use list_dir::ListDir;
pub use read_text::ReadText;
