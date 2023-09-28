A utility to dump OpenShift etcd, requires `go install github.com/rh-ecosystem-edge/ouger/cmd/server@latest`

# Run auth free etcd

Example:

```bash
RELEASE_IMAGE=quay.io/openshift-release-dev/ocp-release:4.13.5-x86_64
ETCD_IMAGE=${ETCD_IMAGE:-"$(oc adm release extract --from="$RELEASE_IMAGE" --file=image-references | jq '.spec.tags[] | select(.name == "etcd").from.name' -r)"}
podman run --network=host --name editor \
    --detach \
    --authfile <your_pull_secret> \
    --entrypoint etcd \
    -v $PWD/backup/var/lib/etcd:/store \
    ${ETCD_IMAGE} --name editor --data-dir /store

until etcdctl endpoint health; do
    sleep 1
done
```

# Run dump utility

```bash
mkdir dump
cargo run --release -- --etcd-endpoint localhost:2379 --output-dir dump
```

