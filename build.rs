use std::{env, fs, io::Write, path::PathBuf};

use anyhow::{Context, Result, anyhow};

#[path = "xtask/src/build_meta_shared.rs"]
mod build_meta_shared;

fn load_cargo_config() -> Result<build_meta_shared::CargoConfig> {
    let toml = fs::read_to_string("Cargo.toml")?;
    Ok(toml::from_str(&toml)?)
}

fn main() -> Result<()> {
    println!("cargo:rerun-if-changed=Cargo.lock");
    println!("cargo:rerun-if-changed=Cargo.toml");
    println!("cargo:rerun-if-changed=.git");
    println!("cargo:rerun-if-changed=xtask/src/build_meta_shared.rs");
    println!("cargo:rerun-if-changed=src/defs.rs");

    let data = load_cargo_config()?;

    gen_kasumi_uapi_bindings()?;
    gen_module_prop(&data)?;

    Ok(())
}

fn kasumi_uapi_header_path() -> PathBuf {
    PathBuf::from("src/sys/kasumi_uapi.h")
}

fn gen_kasumi_uapi_bindings() -> Result<()> {
    let header = kasumi_uapi_header_path();
    let header = fs::canonicalize(&header)
        .with_context(|| format!("failed to resolve Kasumi UAPI header {}", header.display()))?;
    println!("cargo:rerun-if-changed={}", header.display());

    let out_dir = PathBuf::from(
        env::var_os("OUT_DIR").ok_or_else(|| anyhow!("OUT_DIR is not set for build script"))?,
    );
    let bindgen_header = out_dir.join("kasumi_uapi_bindgen.h");
    let wrapper = out_dir.join("kasumi_uapi_wrapper.h");
    let bindings = out_dir.join("kasumi_uapi.rs");

    write_bindgen_header(&header, &bindgen_header)?;

    fs::write(
        &wrapper,
        format!(
            r#"#include <stdint.h>
#include <stddef.h>
#ifndef __u32
typedef uint32_t __u32;
#endif
#ifndef __aligned_u64
typedef uint64_t __aligned_u64;
#endif
#include "{}"
"#,
            bindgen_header.display()
        ),
    )?;

    bindgen::Builder::default()
        .header(wrapper.to_string_lossy())
        .allowlist_type("kasumi_.*")
        .allowlist_var("KSM_MAGIC[12]")
        .allowlist_var("KSM_PROTOCOL_VERSION")
        .allowlist_var("KSM_MAX_LEN_PATHNAME")
        .allowlist_var("KSM_FAKE_CMDLINE_SIZE")
        .allowlist_var("KSM_UNAME_LEN")
        .allowlist_var("KSM_SYSCALL_NR")
        .allowlist_var("KSM_CMD_GET_FD")
        .allowlist_var("KSM_PRCTL_GET_FD")
        .allowlist_var("KSM_FEATURE_.*")
        .allowlist_var("KSM_IOC_MAGIC")
        .derive_debug(true)
        .derive_copy(true)
        .derive_default(false)
        .layout_tests(false)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .map_err(|err| anyhow!("failed to generate Kasumi UAPI bindings: {err}"))?
        .write_to_file(&bindings)
        .with_context(|| format!("failed to write {}", bindings.display()))?;

    Ok(())
}

fn write_bindgen_header(header: &PathBuf, bindgen_header: &PathBuf) -> Result<()> {
    let uapi = fs::read_to_string(header)
        .with_context(|| format!("failed to read Kasumi UAPI header {}", header.display()))?;
    let mut sanitized = String::new();

    // Rust rebuilds ioctl opcodes with rustix; bindgen only needs the ABI structs and constants.
    for line in uapi.lines() {
        let trimmed = line.trim_start();
        if trimmed.starts_with("#define KSM_IOC_") && !trimmed.starts_with("#define KSM_IOC_MAGIC")
        {
            continue;
        }
        sanitized.push_str(line);
        sanitized.push('\n');
    }

    fs::write(bindgen_header, sanitized)
        .with_context(|| format!("failed to write {}", bindgen_header.display()))?;
    Ok(())
}

fn gen_module_prop(data: &build_meta_shared::CargoConfig) -> Result<()> {
    let package = &data.package;
    let id = package.name.replace('-', "_");
    let version_code = build_meta_shared::calculate_version_code(&package.version)?;
    let author = package.authors.join(" & ");
    let version = format!(
        "{}-{}",
        package.version,
        build_meta_shared::git_commit_count()?
    );
    let rendered_version = format!("v{}", version.trim());
    let content = build_meta_shared::render_module_prop(&build_meta_shared::ModulePropData {
        id: &id,
        name: &package.metadata.hybrid_mount.name,
        version: &rendered_version,
        version_code: &version_code,
        author: &author,
        description: &package.description,
        update_json: &package.metadata.hybrid_mount.update,
    });

    let mut file = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .write(true)
        .open("module/module.prop")?;

    file.write_all(content.as_bytes())?;
    Ok(())
}
