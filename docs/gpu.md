# GPU & Ollama Setup

---

## Docker — NVIDIA

Uncomment the `deploy` block in `docker-compose.yml` under the `ollama` service:

```yaml
deploy:
  resources:
    reservations:
      devices:
        - driver: nvidia
          count: all
          capabilities: [gpu]
```

Requires [nvidia-container-toolkit](https://docs.nvidia.com/datacenter/cloud-native/container-toolkit/install-guide.html) on the host.

---

## Docker — AMD (ROCm / Vulkan)

Uncomment the `devices` and `environment` block under `ollama`:

```yaml
devices:
  - /dev/dri:/dev/dri
environment:
  - OLLAMA_VULKAN=1
```

---

## Pulling a model

```bash
# Docker
docker exec ollama ollama pull qwen2.5-coder:7b-instruct

# Bare metal
ollama pull qwen2.5-coder:7b-instruct
```

**Model selection by RAM:**

| RAM | Recommended model |
|---|---|
| ≥ 32 GB | `qwen2.5-coder:14b-instruct` |
| ≥ 16 GB | `qwen2.5-coder:7b-instruct` |
| ≥ 8 GB | `qwen2.5-coder:3b-instruct` |
| < 8 GB | No auto-pull — configure manually or use a remote endpoint |

---

## Using a remote Ollama instance

Set in `.env` (Docker) or the Odysseus `.env` (bare metal):

```
OLLAMA_BASE_URL=http://192.168.1.5:11434
```

---

## TrueNAS Scale

Ollama is commented out in the YAML by default. To enable it:

- **Option A:** Edit the app YAML in the TrueNAS UI and uncomment the `ollama` service block, then save and restart.
- **Option B:** Uncomment the `ollama` block in `deploy/truenas/custom-app.yml` and run `docker compose ... up -d` again.

For NVIDIA GPU passthrough on TrueNAS Scale, uncomment the `deploy.resources` block under `ollama` and ensure `nvidia-container-toolkit` is installed on the host.
