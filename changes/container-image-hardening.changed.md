The container image now runs as the numeric user `10001:10001`, so Kubernetes `runAsNonRoot` accepts it, and it is built on Debian 13 (trixie) from base images pinned by digest.
