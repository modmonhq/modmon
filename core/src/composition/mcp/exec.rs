//! `mm_exec`: one command in a service container, through the audited
//! application use case, with bounded output.

use super::*;

impl McpApplicationAdapter {
    pub(super) async fn run_exec(
        &self,
        context: &McpRequestContext,
        request: McpExecRequest,
        cancellation: crate::domain::observability::OperationCancellation,
    ) -> Result<McpExecResult, MmError> {
        context.require(AgentAction::Exec, Some(&request.service))?;
        if request.argv.is_empty()
            || request.argv.len() > MCP_MAX_EXEC_ARGUMENTS
            || request.timeout_seconds == 0
            || request.timeout_seconds > MCP_MAX_EXEC_SECONDS
        {
            return Err(MmError::Environment {
                message: format!(
                    "mm_exec takes 1-{MCP_MAX_EXEC_ARGUMENTS} argv values and a timeout of 1-{MCP_MAX_EXEC_SECONDS} seconds"
                ),
            }
            .in_own_words());
        }
        let selection = self.selection(context);
        let runtime = self.runtime();
        let service = request.service.clone();
        let exec_request = ExecRequest {
            selector: ServiceSelector {
                stack: request.stack,
                module: None,
                service: request.service,
                container: None,
            },
            argv: request.argv,
            user: request.user,
            workdir: request.workdir,
            environment: Vec::new(),
            tty: ExecTtyMode::Disabled,
            stdin: ExecStdinMode::Closed,
            timeout: std::time::Duration::from_secs(request.timeout_seconds),
            approval: self.approval(),
            shell_wrapper: false,
        };
        let mut stream = self
            .gateway(&selection, context, &runtime)?
            .exec(exec_request, cancellation.clone())
            .await?;
        collect_output(&mut stream, service).await
    }
}

/// Gathers a command's output up to the byte bound. Past the bound the
/// command is stopped and the result says so.
async fn collect_output(
    stream: &mut crate::app::ports::ExecStream,
    service: String,
) -> Result<McpExecResult, MmError> {
    let mut result = McpExecResult {
        schema_version: MCP_TOOL_SCHEMA_VERSION.to_owned(),
        service,
        exit_code: -1,
        stdout: String::new(),
        stderr: String::new(),
        truncated: false,
    };
    let mut finished = false;
    while let Some(event) = stream.recv().await {
        match event? {
            ExecEvent::Selected { .. } => {}
            ExecEvent::Output { source, value, .. } => {
                let room =
                    MCP_MAX_EXEC_BYTES.saturating_sub(result.stdout.len() + result.stderr.len());
                let buffer = match source {
                    ExecOutputSource::Stdout => &mut result.stdout,
                    ExecOutputSource::Stderr => &mut result.stderr,
                };
                if value.len() > room {
                    let mut end = room;
                    while !value.is_char_boundary(end) {
                        end -= 1;
                    }
                    buffer.push_str(&value[..end]);
                    result.truncated = true;
                    stream.cancel();
                    break;
                }
                buffer.push_str(&value);
            }
            ExecEvent::Finished(outcome) => {
                result.exit_code = outcome.exit_code;
                finished = true;
            }
        }
    }
    if !finished && !result.truncated {
        return Err(MmError::RuntimeResponse {
            message: "exec stream ended without a command result".to_owned(),
        });
    }
    Ok(result)
}
