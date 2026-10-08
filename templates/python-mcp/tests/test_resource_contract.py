"""Owner route types, template agreement, and rejected aliases at the MCP edge."""
import pytest
from veoveo_mcp.contract.artifacts import ArtifactId
from veoveo_mcp.types import ResourceUri
from veoveo_mcp.tasks import new_task_id
from datasheet_mcp import uris
from datasheet_mcp.server.contract import SERVER_SETUP


def test_entire_static_discovery_uses_the_owner_parser():
    assert SERVER_SETUP.scope_names == frozenset()
    for descriptor in SERVER_SETUP.resources():
        address = uris.parse_resource_uri(ResourceUri(descriptor.uri))
        assert address is not None
        assert address.to_uri() == descriptor.uri


def test_templates_expand_into_typed_owner_resources():
    task = new_task_id()
    artifact = ArtifactId(str(task))
    pairs = (
        (uris.USAGE_TASK_TEMPLATE.expand(task_id=str(task)), uris.TaskUsageResource(task)),
        (uris.ARTIFACT_TEMPLATE.expand(artifact_id=artifact), uris.ArtifactResource(artifact)),
        (uris.DOCS_TEMPLATE.expand(doc_id="design"), uris.DocumentResource(uris.DocumentId.DESIGN)),
        (uris.REPORTS_TEMPLATE.expand(), uris.ReportCatalogResource()),
        (uris.USAGE_TEMPLATE.expand(), uris.UsageCatalogResource()),
    )
    for uri, address in pairs:
        assert address.to_uri() == uri
        assert uris.parse_resource_uri(uri) == address
    assert uris.parse_resource_uri(uris.DocumentCatalogResource(uris.DocumentId.AGENTS).to_uri()).after == uris.DocumentId.AGENTS


@pytest.mark.parametrize("uri", [
    "datasheet://docs/%64esign", "datasheet://docs/design?", "datasheet://docs/unknown",
    "datasheet://docs/design/extra", "datasheet://docs/design%2Fextra", "datasheet://reports/",
    "datasheet://reports?", "datasheet://reports?cursor=x&cursor=y", "datasheet://usage/task/%zz",
    "datasheet://artifact/019FFDB2-0598-7476-96D3-F3D7B0769F9E",
    "datasheet://docs:80/design", "datasheet://docs/design#fragment",
])
def test_owner_rejects_unknown_or_noncanonical_resource_spellings(uri):
    try:
        assert uris.parse_resource_uri(ResourceUri(uri)) is None
    except ValueError:
        pass


def test_builders_reject_wrong_identity_and_cursor_types():
    for construct, value in (
        (uris.ArtifactResource, str(new_task_id())),
        (uris.TaskUsageResource, str(new_task_id())),
        (uris.DocumentResource, "design"),
        (uris.DocumentCatalogResource, "design"),
        (uris.ReportCatalogResource, uris.UsageCursor(task_id=new_task_id())),
        (uris.UsageCatalogResource, uris.ReportCursor(task_id=new_task_id(), created_at="2026-10-02T00:00:00Z")),
    ):
        with pytest.raises(TypeError):
            construct(value)


def test_workbench_current_wire_requests_replies_and_bridge_notifications():
    """Execute the packaged script; this CPU control makes no rendering claim."""
    import json
    import re
    import subprocess

    from datasheet_mcp.app import APP_HTML

    script = re.search(r"<script>(.*?)</script>", APP_HTML, re.S).group(1)
    runner = r'''
const vm = require("node:vm"), assert = require("node:assert/strict");
const script = JSON.parse(require("node:fs").readFileSync(0, "utf8"));
async function fixture(replies) {
  const elements = new Map(), requests = [], notifications = [], listeners = new Map();
  const element = id => {
    if (!elements.has(id)) elements.set(id, {value: id === "csv" ? "x\n1" : "x", textContent: "", disabled: id === "more-reports"});
    return elements.get(id);
  };
  const document = {getElementById: element, documentElement: {dataset: {}, scrollHeight: 520}};
  const receive = message => listeners.get("message")({data: message});
  const context = vm.createContext({document, URL, console, setTimeout,
    addEventListener: (event, callback) => listeners.set(event, callback),
    parent: {postMessage(message) {
      if (message.id === undefined || !message.method) {notifications.push(message); return;}
      requests.push(message);
      const result = message.method === "ui/initialize" ? {hostContext: {theme: "dark"}} : replies.shift();
      assert.notEqual(result, undefined, `Unexpected request ${message.method}`);
      queueMicrotask(() => receive({jsonrpc: "2.0", id: message.id, result}));
    }}
  });
  vm.runInContext(script, context, {timeout: 1000});
  await new Promise(resolve => setImmediate(resolve));
  assert.equal(element("status").textContent, "ready");
  return {element, requests, notifications, receive, context};
}
(async () => {
  const current = await fixture([
    {structuredContent: {rows: [{x: 1}]}},
    {structuredContent: {column: "x"}},
    {resultType: "task", taskId: "task-current", status: "working"},
    {resultType: "complete", taskId: "task-current", status: "completed", result: {structuredContent: {rowCount: 1}}},
    {contents: [{text: JSON.stringify({items: [{taskId: "task-current"}], limit: 100, nextCursor: "current", nextUri: "datasheet://reports?cursor=current"})}]},
    {contents: [{text: JSON.stringify({items: [], limit: 100, nextCursor: null, nextUri: null})}]}
  ]);
  await current.element("preview").onclick();
  await current.element("stats").onclick();
  await current.element("profile").onclick();
  const calls = current.requests.filter(request => request.method === "tools/call");
  assert.deepEqual(JSON.parse(JSON.stringify(calls.map(call => call.params))), [
    {name: "preview_dataset", arguments: {inlineCsv: "x\n1", rows: 20}},
    {name: "column_stats", arguments: {inlineCsv: "x\n1", column: "x"}},
    {name: "profile_dataset", arguments: {inlineCsv: "x\n1", artifact: true, histogramBins: 20}}
  ]);
  assert.deepEqual(JSON.parse(JSON.stringify(current.requests.find(request => request.method === "tasks/get").params)), {taskId: "task-current"});
  await current.element("reports").onclick();
  assert.equal(current.element("more-reports").disabled, false);
  await current.element("more-reports").onclick();
  assert.equal(current.requests.at(-1).params.uri, "datasheet://reports?cursor=current");
  assert.equal(current.element("more-reports").disabled, true);
  assert(current.notifications.some(message => message.method === "ui/notifications/initialized"));
  assert(current.notifications.some(message => message.method === "ui/notifications/size-changed" && message.params.height === 520));
  current.receive({jsonrpc: "2.0", method: "ui/notifications/host-context-changed", params: {hostContext: {theme: "light"}}});
  assert.equal(current.context.document.documentElement.dataset.theme, "light");
  current.receive({jsonrpc: "2.0", id: 100, method: "ui/resource-teardown", params: {}});
  assert.equal(current.notifications.at(-1).id, 100);

  for (const reply of [
    {resultType: "task", task_id: "retired"},
    {resultType: "task", taskId: "current", task_id: "retired"},
    {structured_content: {rows: []}},
    {structuredContent: {rows: []}, structured_content: {rows: []}}
  ]) {
    const rejected = await fixture([reply]);
    await rejected.element("profile").onclick();
    assert.equal(rejected.element("status").textContent, "failed");
    assert.equal(rejected.requests.filter(request => request.method === "tasks/get").length, 0);
  }
  for (const reply of [
    {resultType: "complete", taskId: "task-current", state: "completed"},
    {resultType: "complete", taskId: "task-current", status: "completed", state: "completed"},
    {resultType: "complete", taskId: "task-current", status: "completed", task_id: "retired"}
  ]) {
    const rejected = await fixture([{resultType: "task", taskId: "task-current"}, reply]);
    await rejected.element("profile").onclick();
    assert.equal(rejected.element("status").textContent, "failed");
    assert.equal(rejected.notifications.filter(message => message.method === "ui/notifications/size-changed").length, 0);
  }
  for (const page of [
    {items: [], limit: 100, next_cursor: "retired", next_uri: "datasheet://reports?cursor=retired"},
    {items: [], limit: 100, nextCursor: "current", nextUri: "datasheet://reports?cursor=current", next_uri: "datasheet://reports?cursor=retired"}
  ]) {
    const rejected = await fixture([{contents: [{text: JSON.stringify(page)}]}]);
    await rejected.element("reports").onclick();
    assert.equal(rejected.element("output").textContent, "Invalid report page");
    assert.equal(rejected.element("more-reports").disabled, true);
    assert.equal(rejected.requests.filter(request => request.method === "resources/read").length, 1);
  }
})().catch(error => {console.error(error); process.exitCode = 1;});
'''
    result = subprocess.run(
        ["node", "-e", runner], input=json.dumps(script), text=True,
        capture_output=True, timeout=10, check=False,
    )
    assert result.returncode == 0, result.stderr


def test_prompt_arguments_share_advertised_current_wire_and_refuse_retired_keys():
    from datasheet_mcp.prompts import PROFILE_PROMPT, REVIEW_PROMPT, get_prompt, list_prompts

    task = new_task_id()
    advertised = {prompt.name: [argument.name for argument in prompt.arguments] for prompt in list_prompts()}
    assert advertised == {PROFILE_PROMPT: ["datasetUri"], REVIEW_PROMPT: ["taskId"]}
    dataset_uri = str(uris.ArtifactResource(ArtifactId(str(task))).to_uri())
    profile = get_prompt(PROFILE_PROMPT, {"datasetUri": dataset_uri})
    assert dataset_uri in profile.messages[0].content.text
    review = get_prompt(REVIEW_PROMPT, {"taskId": str(task)})
    assert str(uris.TaskUsageResource(task).to_uri()) in review.messages[0].content.text
    for name, current, old, value in (
        (PROFILE_PROMPT, "datasetUri", "dataset_uri", dataset_uri),
        (REVIEW_PROMPT, "taskId", "task_id", str(task)),
    ):
        for arguments in ({old: value}, {old: value, current: value}, {}, {current: value, "unknown": value}):
            with pytest.raises(ValueError):
                get_prompt(name, arguments)
