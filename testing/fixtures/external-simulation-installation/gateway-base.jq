def selected_server:
  . as $slug
  | ["artifact", "frames", "simulation-view"]
  | index($slug) != null;

walk(
  if type == "string" then
    gsub("https://veoveo.example"; "http://localhost:8782")
  else
    .
  end
)
| .identity_providers[0].jwks = {
    "source": "file",
    "path": "/etc/veoveo/gateway/jwks.json"
  }
| .authorization_servers[0].jwks = {
    "source": "file",
    "path": "/etc/veoveo/gateway/jwks.json"
  }
| .servers |= map(select(.slug | selected_server))
| .profiles |= map(
    select(.id == "operator")
    | .protected_resource = "http://localhost:8782/mcp/operator"
    | .auth_modes = ["oauth_client_credentials"]
    | .required_scopes = [
        "operator:use",
        "simulation-view:read",
        "simulation-view:write",
        "simulation-view:stream"
      ]
    | .servers |= map(select(.server | selected_server))
  )
| .policies[0].rules |= map(
    select(
      ((.servers // []) | any(selected_server))
      and ((.profiles // []) | index("operator") != null)
    )
    | .profiles |= map(select(. == "operator"))
  )
| .recording_ingest_resources = []
| .oidc_clients = []
| .oauth_clients |= map(
    select(.id == "operator-service")
    | .allowed_resources = ["http://localhost:8782/mcp/operator"]
    | .allowed_scopes = [
        "operator:use",
        "simulation-view:read",
        "simulation-view:write",
        "simulation-view:stream"
      ]
    | .jwks = {
        "source": "file",
        "path": "/etc/veoveo/gateway/jwks.json"
      }
  )
| .work_contexts |= map(
    select(.id == "operations")
    | .memberships = [
        {
          "level": "contributor",
          "oauth_clients": ["operator-service"]
        }
      ]
  )
| .secrets |= map(select(.owner.kind == "gateway"))
| .metadata = {
    "deployment": "anonymous-simulation",
    "environment": "loopback_acceptance"
  }
