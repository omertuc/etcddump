A utility to dump OpenShift etcd, requires `go install github.com/rh-ecosystem-edge/ouger/cmd/server@latest`

# Run

```bash
mkdir dump
cargo run --release -- --etcd-endpoint localhost:2379 --output-dir dump
```

