use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use near_core::{
    ActionContext, ArgumentKind, ArgumentSchema, CommandDescriptor, CommandExtension, CommandId,
    CommandInvocation, CommandPrefixDescriptor, ExtensionCommandPrefix, ExtensionEffect,
    ExtensionReport, Location, ProviderId, ResourceRef, SafetyClass,
};
use semver::{Version, VersionReq};
use serde::{Deserialize, Serialize};

use crate::{
    CapabilityGrantStore, PluginCommandPrefix, PluginError, PluginOrigin, validate_artifact,
    validate_id, validate_plugin_prefixes,
};

pub const PROCESS_PROTOCOL_VERSION: &str = "0.1.0";

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessPluginManifest {
    pub schema: u32,
    pub id: String,
    pub name: String,
    pub version: Version,
    pub protocol: VersionReq,
    pub runtime: String,
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub capabilities: BTreeSet<String>,
    #[serde(default)]
    pub commands: Vec<ProcessCommandManifest>,
    #[serde(default)]
    pub prefixes: Vec<PluginCommandPrefix>,
    #[serde(default)]
    pub limits: ProcessLimits,
}

impl ProcessPluginManifest {
    /// Parses and validates a process-extension manifest.
    ///
    /// # Errors
    ///
    /// Returns TOML, version, artifact, capability, command, or limit failures.
    pub fn from_toml(document: &str) -> Result<Self, PluginError> {
        let manifest: Self = toml::from_str(document)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// Validates the process protocol and deny-ambient-authority v1 contract.
    ///
    /// # Errors
    ///
    /// Returns the first incompatible or unsafe field.
    pub fn validate(&self) -> Result<(), PluginError> {
        if self.schema != 1 {
            return Err(PluginError::UnsupportedManifestSchema(self.schema));
        }
        validate_id(&self.id)?;
        if self.runtime != "process" {
            return Err(PluginError::InvalidComponent(format!(
                "process manifest runtime must be process, found {}",
                self.runtime
            )));
        }
        validate_artifact(&self.executable)?;
        if !self.protocol.matches(&Version::new(0, 1, 0)) {
            return Err(PluginError::IncompatibleInterface {
                required: self.protocol.to_string(),
                provided: PROCESS_PROTOCOL_VERSION.to_owned(),
            });
        }
        if let Some(capability) = self.capabilities.first() {
            return Err(PluginError::UnknownCapability(format!(
                "process protocol v0.1 has no direct ambient capability: {capability}"
            )));
        }
        if self.limits.timeout_ms == 0
            || self.limits.max_request_bytes == 0
            || self.limits.max_output_bytes == 0
        {
            return Err(PluginError::InvalidComponent(
                "process timeout, request, and output limits must be positive".to_owned(),
            ));
        }
        let mut commands = BTreeSet::new();
        for command in &self.commands {
            validate_id(&command.id)?;
            if !commands.insert(command.id.clone()) {
                return Err(PluginError::InvalidComponent(format!(
                    "duplicate process command {}",
                    command.id
                )));
            }
            command.safety_class()?;
        }
        validate_plugin_prefixes(
            &self.prefixes,
            &self
                .commands
                .iter()
                .map(|command| command.id.clone())
                .collect::<Vec<_>>(),
        )?;
        Ok(())
    }
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessCommandManifest {
    pub id: String,
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub category: Vec<String>,
    pub safety: String,
}

impl ProcessCommandManifest {
    fn safety_class(&self) -> Result<SafetyClass, PluginError> {
        match self.safety.as_str() {
            "read-only" => Ok(SafetyClass::ReadOnly),
            "confirmable" => Ok(SafetyClass::Confirmable),
            "destructive" => Ok(SafetyClass::Destructive),
            other => Err(PluginError::InvalidComponent(format!(
                "unknown process command safety class {other}"
            ))),
        }
    }

    fn descriptor(&self) -> Result<CommandDescriptor, PluginError> {
        Ok(CommandDescriptor {
            id: CommandId::from(self.id.clone()),
            title: self.title.clone(),
            description: self.description.clone(),
            category: self.category.clone(),
            safety: self.safety_class()?,
            arguments: BTreeMap::new(),
        })
    }
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub struct ProcessLimits {
    pub timeout_ms: u64,
    #[serde(default = "default_max_request_bytes")]
    pub max_request_bytes: usize,
    pub max_output_bytes: usize,
}

impl Default for ProcessLimits {
    fn default() -> Self {
        Self {
            timeout_ms: 1_000,
            max_request_bytes: default_max_request_bytes(),
            max_output_bytes: 1024 * 1024,
        }
    }
}

const fn default_max_request_bytes() -> usize {
    1024 * 1024
}

pub struct ProcessPluginHost {
    grants: CapabilityGrantStore,
}

impl ProcessPluginHost {
    pub fn new(grants: CapabilityGrantStore) -> Self {
        Self { grants }
    }

    /// Loads a compiled process extension from a package directory.
    ///
    /// # Errors
    ///
    /// Returns manifest, workspace trust, path containment, executable, or platform failures.
    pub fn load(
        &self,
        manifest: ProcessPluginManifest,
        origin: PluginOrigin,
        package: &Path,
    ) -> Result<ProcessPlugin, PluginError> {
        manifest.validate()?;
        if origin == PluginOrigin::Workspace && !self.grants.permits_workspace(&manifest.id) {
            return Err(PluginError::WorkspaceTrustRequired(manifest.id));
        }
        if !cfg!(target_os = "macos") {
            return Err(PluginError::Runtime(
                "process extension sandbox is currently available only on macOS".to_owned(),
            ));
        }
        let package = package
            .canonicalize()
            .map_err(|error| PluginError::Runtime(error.to_string()))?;
        let executable = package
            .join(&manifest.executable)
            .canonicalize()
            .map_err(|error| PluginError::Runtime(error.to_string()))?;
        if !executable.starts_with(&package) || !executable.is_file() {
            return Err(PluginError::InvalidArtifact(manifest.executable.clone()));
        }
        Ok(ProcessPlugin {
            manifest,
            package,
            executable,
        })
    }
}

pub struct ProcessPlugin {
    manifest: ProcessPluginManifest,
    package: PathBuf,
    executable: PathBuf,
}

impl ProcessPlugin {
    pub fn manifest(&self) -> &ProcessPluginManifest {
        &self.manifest
    }

    fn run(&self, request: &ProcessRequest) -> Result<ProcessResponse, PluginError> {
        let request = serde_json::to_vec(request)
            .map_err(|error| PluginError::Serialization(error.to_string()))?;
        if request.len() > self.manifest.limits.max_request_bytes {
            return Err(PluginError::Runtime(format!(
                "process extension {} request exceeded {} bytes",
                self.manifest.id, self.manifest.limits.max_request_bytes
            )));
        }
        let profile = sandbox_profile(&self.package, &self.executable);
        let mut command = Command::new("/usr/bin/sandbox-exec");
        command
            .arg("-p")
            .arg(profile)
            .arg(&self.executable)
            .args(&self.manifest.arguments)
            .env_clear()
            .env("NEAR_PROCESS_PROTOCOL", PROCESS_PROTOCOL_VERSION)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(PluginError::runtime)?;
        let overflow = Arc::new(AtomicBool::new(false));
        let stdout = child.stdout.take().ok_or_else(|| {
            PluginError::Runtime("process extension stdout is unavailable".to_owned())
        })?;
        let stderr = child.stderr.take().ok_or_else(|| {
            PluginError::Runtime("process extension stderr is unavailable".to_owned())
        })?;
        let stdout_overflow = Arc::clone(&overflow);
        let stderr_overflow = Arc::clone(&overflow);
        let limit = self.manifest.limits.max_output_bytes;
        let stdout_thread = thread::spawn(move || read_bounded(stdout, limit, &stdout_overflow));
        let stderr_thread = thread::spawn(move || read_bounded(stderr, limit, &stderr_overflow));
        let mut stdin = child.stdin.take().ok_or_else(|| {
            PluginError::Runtime("process extension stdin is unavailable".to_owned())
        })?;
        let stdin_thread = thread::spawn(move || {
            stdin.write_all(&request)?;
            stdin.write_all(b"\n")
        });

        let deadline =
            Instant::now() + Duration::from_millis(self.manifest.limits.timeout_ms.max(1));
        let mut timed_out = false;
        let status = loop {
            if let Some(status) = child.try_wait().map_err(PluginError::runtime)? {
                break status;
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                timed_out = true;
                break child.wait().map_err(PluginError::runtime)?;
            }
            thread::sleep(Duration::from_millis(5));
        };
        let stdout = stdout_thread
            .join()
            .map_err(|_| PluginError::Runtime("stdout reader panicked".to_owned()))?;
        let stderr = stderr_thread
            .join()
            .map_err(|_| PluginError::Runtime("stderr reader panicked".to_owned()))?;
        let stdin_result = stdin_thread
            .join()
            .map_err(|_| PluginError::Runtime("stdin writer panicked".to_owned()))?;
        if timed_out {
            return Err(PluginError::Runtime(format!(
                "process extension {} exceeded {} ms",
                self.manifest.id, self.manifest.limits.timeout_ms
            )));
        }
        if overflow.load(Ordering::Acquire) {
            return Err(PluginError::Runtime(format!(
                "process extension {} exceeded output limit",
                self.manifest.id
            )));
        }
        if !status.success() {
            return Err(PluginError::Runtime(format!(
                "process extension {} exited with {status}: {}",
                self.manifest.id,
                String::from_utf8_lossy(&stderr).trim()
            )));
        }
        stdin_result.map_err(PluginError::runtime)?;
        serde_json::from_slice(&stdout)
            .map_err(|error| PluginError::Serialization(error.to_string()))
    }
}

impl CommandExtension for ProcessPlugin {
    fn id(&self) -> &str {
        &self.manifest.id
    }

    fn commands(&self) -> Result<Vec<CommandDescriptor>, String> {
        let mut commands: Vec<CommandDescriptor> = self
            .manifest
            .commands
            .iter()
            .map(ProcessCommandManifest::descriptor)
            .collect::<Result<_, _>>()
            .map_err(|error| error.to_string())?;
        for prefix in &self.manifest.prefixes {
            let command = commands
                .iter_mut()
                .find(|command| command.id.as_str() == prefix.command)
                .expect("validated process prefix command exists");
            command
                .arguments
                .entry(prefix.argument.clone())
                .or_insert(ArgumentSchema {
                    kind: ArgumentKind::String,
                    required: false,
                    description: format!("Arguments supplied through {}:", prefix.name),
                    default: None,
                });
        }
        Ok(commands)
    }

    fn command_prefixes(&self) -> Result<Vec<ExtensionCommandPrefix>, String> {
        Ok(self
            .manifest
            .prefixes
            .iter()
            .map(|prefix| ExtensionCommandPrefix {
                prefix: CommandPrefixDescriptor {
                    name: prefix.name.clone(),
                    description: prefix.description.clone(),
                },
                command: CommandId::from(prefix.command.clone()),
                argument: prefix.argument.clone(),
            })
            .collect())
    }

    fn invoke(
        &self,
        invocation: &CommandInvocation,
        context: &ActionContext,
    ) -> Result<ExtensionReport, String> {
        if !self
            .manifest
            .commands
            .iter()
            .any(|command| command.id == invocation.id.as_str())
        {
            return Err(format!(
                "process extension {} does not own command {}",
                self.manifest.id, invocation.id
            ));
        }
        let request = ProcessRequest {
            schema: 1,
            kind: "invoke",
            command: invocation.id.as_str(),
            context: ProcessContext::from(context),
            arguments: &invocation.arguments,
        };
        let response = self.run(&request).map_err(|error| error.to_string())?;
        if response.schema != 1 {
            return Err(format!(
                "unsupported process response schema {}",
                response.schema
            ));
        }
        let effect = match response.effect {
            ProcessEffect::Message { value } => ExtensionEffect::Message(value),
            ProcessEffect::Navigate { location } => {
                ExtensionEffect::Navigate(Location::new(location))
            }
            ProcessEffect::Open { resources } => ExtensionEffect::Open(
                resources
                    .into_iter()
                    .map(|resource| ResourceRef {
                        provider: ProviderId::from(resource.provider),
                        location: Location::new(resource.uri),
                    })
                    .collect(),
            ),
            ProcessEffect::Task { id } => ExtensionEffect::Task(id),
        };
        Ok(ExtensionReport {
            effect,
            diagnostics: response.diagnostics,
        })
    }
}

#[derive(Serialize)]
struct ProcessRequest<'a> {
    schema: u32,
    kind: &'static str,
    command: &'a str,
    context: ProcessContext,
    arguments: &'a BTreeMap<String, near_core::CommandValue>,
}

#[derive(Serialize)]
struct ProcessContext {
    focused_location: Option<String>,
    peer_location: Option<String>,
    current: Option<ProcessResource>,
    selected: Vec<ProcessResource>,
    capabilities: Vec<String>,
}

impl From<&ActionContext> for ProcessContext {
    fn from(context: &ActionContext) -> Self {
        Self {
            focused_location: context
                .location
                .as_ref()
                .map(|location| location.as_str().to_owned()),
            peer_location: context
                .peer_location
                .as_ref()
                .map(|location| location.as_str().to_owned()),
            current: context.current.as_ref().map(ProcessResource::from),
            selected: context.selected.iter().map(ProcessResource::from).collect(),
            capabilities: context
                .capabilities
                .iter()
                .map(|capability| capability.as_str().to_owned())
                .collect(),
        }
    }
}

#[derive(Deserialize, Serialize)]
struct ProcessResource {
    provider: String,
    uri: String,
}

impl From<&ResourceRef> for ProcessResource {
    fn from(resource: &ResourceRef) -> Self {
        Self {
            provider: resource.provider.as_str().to_owned(),
            uri: resource.location.as_str().to_owned(),
        }
    }
}

#[derive(Deserialize)]
struct ProcessResponse {
    schema: u32,
    effect: ProcessEffect,
    #[serde(default)]
    diagnostics: Vec<String>,
}

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
enum ProcessEffect {
    Message { value: String },
    Navigate { location: String },
    Open { resources: Vec<ProcessResource> },
    Task { id: String },
}

fn sandbox_profile(package: &Path, executable: &Path) -> String {
    format!(
        "(version 1)\n(deny default)\n(import \"system.sb\")\n(allow process-exec (literal {}))\n(allow file-read* (literal {}) (subpath {}))\n(allow file-write* (literal \"/dev/stdout\") (literal \"/dev/stderr\"))",
        sandbox_string(executable),
        sandbox_string(executable),
        sandbox_string(package),
    )
}

fn sandbox_string(path: &Path) -> String {
    let value = path.to_string_lossy();
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

fn read_bounded(mut reader: impl Read, limit: usize, overflow: &AtomicBool) -> Vec<u8> {
    let mut output = Vec::new();
    let mut buffer = [0_u8; 8 * 1024];
    loop {
        match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(read) => {
                let remaining = limit.saturating_sub(output.len());
                output.extend_from_slice(&buffer[..read.min(remaining)]);
                if read > remaining {
                    overflow.store(true, Ordering::Release);
                }
            }
        }
    }
    output
}

#[cfg(test)]
mod tests {
    #[cfg(target_os = "macos")]
    use std::{
        fs,
        time::{SystemTime, UNIX_EPOCH},
    };

    use super::*;

    #[test]
    fn process_manifest_rejects_direct_ambient_capabilities() {
        let manifest = ProcessPluginManifest {
            schema: 1,
            id: "test.process".to_owned(),
            name: "Test".to_owned(),
            version: Version::new(1, 0, 0),
            protocol: VersionReq::parse("^0.1").unwrap(),
            runtime: "process".to_owned(),
            executable: "plugin".to_owned(),
            arguments: Vec::new(),
            capabilities: BTreeSet::from(["near.fs.read@1".to_owned()]),
            commands: Vec::new(),
            prefixes: Vec::new(),
            limits: ProcessLimits::default(),
        };
        assert!(matches!(
            manifest.validate(),
            Err(PluginError::UnknownCapability(_))
        ));
    }

    #[test]
    fn process_manifest_rejects_incompatible_protocol_versions() {
        let manifest = ProcessPluginManifest {
            schema: 1,
            id: "test.process".to_owned(),
            name: "Test".to_owned(),
            version: Version::new(1, 0, 0),
            protocol: VersionReq::parse("^0.2").unwrap(),
            runtime: "process".to_owned(),
            executable: "plugin".to_owned(),
            arguments: Vec::new(),
            capabilities: BTreeSet::new(),
            commands: Vec::new(),
            prefixes: Vec::new(),
            limits: ProcessLimits::default(),
        };
        assert!(matches!(
            manifest.validate(),
            Err(PluginError::IncompatibleInterface { .. })
        ));
    }

    #[test]
    fn sandbox_profile_quotes_paths_and_remains_deny_default() {
        let profile = sandbox_profile(
            Path::new("/tmp/package with space"),
            Path::new("/tmp/package with space/plugin\"name"),
        );
        assert!(profile.contains("(deny default)"));
        assert!(profile.contains("(import \"system.sb\")"));
        assert!(profile.contains("plugin\\\"name"));
        assert!(!profile.contains("allow network"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires macOS sandbox-exec outside a nested sandbox"]
    fn macos_sandbox_denies_external_files_and_environment() {
        let listener = std::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        let package = TestPackage::compile_with("isolation", |root| {
            let secret = root.with_extension("secret");
            format!(
                r#"#include <stdio.h>
#include <stdlib.h>
#include <arpa/inet.h>
#include <netinet/in.h>
#include <spawn.h>
#include <sys/socket.h>
#include <unistd.h>
extern char **environ;
int main(void) {{
    FILE *secret = fopen("{}", "r");
    const char *home = getenv("HOME");
    int network = socket(AF_INET, SOCK_STREAM, 0);
    struct sockaddr_in address = {{0}};
    address.sin_family = AF_INET;
    address.sin_port = htons({port});
    address.sin_addr.s_addr = htonl(INADDR_LOOPBACK);
    int connected = network == -1 ? -1 : connect(network, (struct sockaddr *)&address, sizeof(address));
    pid_t child = 0;
    char *arguments[] = {{ "/bin/true", NULL }};
    int spawned = posix_spawn(&child, "/bin/true", NULL, NULL, arguments, environ);
    printf("{{\"schema\":1,\"effect\":{{\"kind\":\"message\",\"value\":\"file=%d env=%d connect=%d spawn=%d\"}}}}\n", secret != NULL, home != NULL, connected, spawned);
    if (secret != NULL) fclose(secret);
    if (network != -1) close(network);
    return 0;
}}
"#,
                c_string(&secret)
            )
        });
        let secret = package.root.with_extension("secret");
        fs::write(&secret, "not extension authority").unwrap();
        let report = invoke(&package, ProcessLimits::default()).unwrap();
        let ExtensionEffect::Message(result) = report.effect else {
            panic!("expected process diagnostic message");
        };
        assert_eq!(result, "file=0 env=0 connect=-1 spawn=1");
        fs::remove_file(secret).unwrap();
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires macOS sandbox-exec outside a nested sandbox"]
    fn macos_process_crash_is_isolated_and_next_invocation_recovers() {
        let crashing = TestPackage::compile(
            "crash",
            "#include <stdlib.h>\nint main(void) { abort(); }\n",
        );
        let error = invoke(&crashing, ProcessLimits::default()).unwrap_err();
        assert!(error.contains("exited with"), "{error}");

        let healthy = TestPackage::compile("healthy", message_program("healthy"));
        let report = invoke(&healthy, ProcessLimits::default()).unwrap();
        assert_eq!(
            report.effect,
            ExtensionEffect::Message("healthy".to_owned())
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires macOS sandbox-exec outside a nested sandbox"]
    fn macos_process_timeout_is_killed() {
        let package = TestPackage::compile("timeout", "int main(void) { for (;;) {} }\n");
        let limits = ProcessLimits {
            timeout_ms: 100,
            ..ProcessLimits::default()
        };
        let started = Instant::now();
        let error = invoke(&package, limits).unwrap_err();
        assert!(error.contains("exceeded 100 ms"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires macOS sandbox-exec outside a nested sandbox"]
    fn macos_process_timeout_covers_a_blocked_request_writer() {
        let package = TestPackage::compile("blocked-stdin", "int main(void) { for (;;) {} }\n");
        let limits = ProcessLimits {
            timeout_ms: 100,
            max_request_bytes: 512 * 1024,
            ..ProcessLimits::default()
        };
        let started = Instant::now();
        let error = invoke_with_arguments(
            &package,
            limits,
            BTreeMap::from([(
                "payload".to_owned(),
                near_core::CommandValue::String("x".repeat(256 * 1024)),
            )]),
        )
        .unwrap_err();
        assert!(error.contains("exceeded 100 ms"), "{error}");
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[cfg(target_os = "macos")]
    #[test]
    #[ignore = "requires macOS sandbox-exec outside a nested sandbox"]
    fn macos_process_output_is_bounded() {
        let package = TestPackage::compile(
            "output",
            "#include <stdio.h>\nint main(void) { for (int i = 0; i < 4096; ++i) putchar('x'); }\n",
        );
        let limits = ProcessLimits {
            max_output_bytes: 128,
            ..ProcessLimits::default()
        };
        let error = invoke(&package, limits).unwrap_err();
        assert!(error.contains("exceeded output limit"), "{error}");
    }

    #[cfg(target_os = "macos")]
    struct TestPackage {
        root: PathBuf,
    }

    #[cfg(target_os = "macos")]
    impl TestPackage {
        fn compile(name: &str, source: impl AsRef<str>) -> Self {
            Self::compile_with(name, |_| source.as_ref().to_owned())
        }

        fn compile_with(name: &str, source: impl FnOnce(&Path) -> String) -> Self {
            let root = unique_test_root(name);
            fs::create_dir_all(&root).unwrap();
            let source_path = root.join("plugin.c");
            fs::write(&source_path, source(&root)).unwrap();
            let status = Command::new("/usr/bin/clang")
                .args(["-std=c11", "-Wall", "-Werror"])
                .arg(&source_path)
                .arg("-o")
                .arg(root.join("plugin"))
                .status()
                .unwrap();
            assert!(status.success());
            Self { root }
        }
    }

    #[cfg(target_os = "macos")]
    impl Drop for TestPackage {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

    #[cfg(target_os = "macos")]
    fn unique_test_root(name: &str) -> PathBuf {
        let nonce = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "near-process-{name}-{}-{nonce}",
            std::process::id()
        ))
    }

    #[cfg(target_os = "macos")]
    fn c_string(path: &Path) -> String {
        path.to_string_lossy()
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
    }

    #[cfg(target_os = "macos")]
    fn message_program(message: &str) -> String {
        format!(
            "#include <stdio.h>\nint main(void) {{ puts(\"{{\\\"schema\\\":1,\\\"effect\\\":{{\\\"kind\\\":\\\"message\\\",\\\"value\\\":\\\"{message}\\\"}}}}\"); }}\n"
        )
    }

    #[cfg(target_os = "macos")]
    fn invoke(package: &TestPackage, limits: ProcessLimits) -> Result<ExtensionReport, String> {
        invoke_with_arguments(package, limits, BTreeMap::new())
    }

    #[cfg(target_os = "macos")]
    fn invoke_with_arguments(
        package: &TestPackage,
        limits: ProcessLimits,
        arguments: BTreeMap<String, near_core::CommandValue>,
    ) -> Result<ExtensionReport, String> {
        let manifest = ProcessPluginManifest {
            schema: 1,
            id: "test.process".to_owned(),
            name: "Test".to_owned(),
            version: Version::new(1, 0, 0),
            protocol: VersionReq::parse("^0.1").unwrap(),
            runtime: "process".to_owned(),
            executable: "plugin".to_owned(),
            arguments: Vec::new(),
            capabilities: BTreeSet::new(),
            commands: vec![ProcessCommandManifest {
                id: "test.process.invoke".to_owned(),
                title: "Invoke".to_owned(),
                description: "Invoke test process".to_owned(),
                category: vec!["Test".to_owned()],
                safety: "read-only".to_owned(),
            }],
            prefixes: Vec::new(),
            limits,
        };
        let plugin = ProcessPluginHost::new(CapabilityGrantStore::default())
            .load(manifest, PluginOrigin::Installed, &package.root)
            .unwrap();
        plugin.invoke(
            &CommandInvocation {
                id: CommandId::from("test.process.invoke"),
                arguments,
            },
            &ActionContext::default(),
        )
    }
}
