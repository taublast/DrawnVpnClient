// The engine (ios/engine/engine.go, a Go C archive): what Provider.swift calls.
#include <stdint.h>

/// Starts Xray with `config`, its TUN inbound on `fd`. NULL, or the error (free it with EngineFree).
char *EngineStart(char *config, int fd);
void EngineStop(void);
/// A request through the running Xray: milliseconds, or -1 for no answer.
long long EngineMeasure(char *url, int timeoutMs);
/// "Xray-core v26.11.9"; free it with EngineFree.
char *EngineVersion(void);
void EngineFree(char *text);
