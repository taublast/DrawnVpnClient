// The engine of the iOS connection extension: Xray-core with its own TUN inbound on the utun fd that
// NetworkExtension gives the extension, built as a C archive (ios/ios.sh) and called from Swift.
// The same JSON config as on Android (src/link.rs, Server::xray_config).
package main

/*
#include <stdlib.h>
*/
import "C"

import (
	"context"
	"io"
	"net"
	"net/http"
	"os"
	"runtime/debug"
	"strconv"
	"strings"
	"sync"
	"time"
	"unsafe"

	"github.com/xtls/xray-core/common/platform"
	xnet "github.com/xtls/xray-core/common/net"
	"github.com/xtls/xray-core/core"
	_ "github.com/xtls/xray-core/main/distro/all"
)

var (
	mu       sync.Mutex
	instance *core.Instance
	freeOnce sync.Once
)

// An extension gets 50 MB in all: Go's heap is kept well under it, and freed memory goes back to
// the system every second (as libXray does on iOS).
const memoryLimit = 30 << 20

func limitMemory() {
	freeOnce.Do(func() {
		debug.SetGCPercent(10)
		debug.SetMemoryLimit(memoryLimit)
		go func() {
			for {
				time.Sleep(time.Second)
				debug.FreeOSMemory()
			}
		}()
	})
}

func cError(err error) *C.char {
	if err == nil {
		return nil
	}
	return C.CString(err.Error())
}

// EngineStart starts Xray with `config`, its TUN inbound on `fd`. Returns NULL, or the error (free it
// with EngineFree).
//
//export EngineStart
func EngineStart(config *C.char, fd C.int) *C.char {
	mu.Lock()
	defer mu.Unlock()
	if instance != nil {
		_ = instance.Close()
		instance = nil
	}
	limitMemory()
	// Xray works on a duplicate of the fd; the fd itself stays NetworkExtension's.
	_ = os.Setenv(platform.TunFdKey, strconv.Itoa(int(fd)))
	cfg, err := core.LoadConfig("json", strings.NewReader(C.GoString(config)))
	if err != nil {
		return cError(err)
	}
	server, err := core.New(cfg)
	if err != nil {
		return cError(err)
	}
	if err = server.Start(); err != nil {
		_ = server.Close()
		return cError(err)
	}
	instance = server
	debug.FreeOSMemory()
	return nil
}

// EngineStop stops Xray; nothing when it is not running.
//
//export EngineStop
func EngineStop() {
	mu.Lock()
	defer mu.Unlock()
	if instance != nil {
		_ = instance.Close()
		instance = nil
	}
	debug.FreeOSMemory()
}

// EngineMeasure: a request to `url` through the running Xray (its default outbound, the server), as
// Android's measureDelay. Milliseconds, or -1 when it got no answer or Xray is not running.
//
//export EngineMeasure
func EngineMeasure(url *C.char, timeoutMs C.int) C.longlong {
	mu.Lock()
	server := instance
	mu.Unlock()
	if server == nil {
		return -1
	}
	timeout := time.Duration(timeoutMs) * time.Millisecond
	transport := &http.Transport{
		DialContext: func(ctx context.Context, network, addr string) (net.Conn, error) {
			dest, err := xnet.ParseDestination(network + ":" + addr)
			if err != nil {
				return nil, err
			}
			return core.Dial(ctx, server, dest)
		},
		TLSHandshakeTimeout: timeout,
		DisableKeepAlives:   true,
	}
	defer transport.CloseIdleConnections()
	client := &http.Client{Transport: transport, Timeout: timeout}
	start := time.Now()
	resp, err := client.Get(C.GoString(url))
	if err != nil {
		return -1
	}
	_, _ = io.Copy(io.Discard, resp.Body)
	_ = resp.Body.Close()
	return C.longlong(time.Since(start).Milliseconds())
}

// EngineVersion: "Xray-core v26.11.9"; free it with EngineFree.
//
//export EngineVersion
func EngineVersion() *C.char {
	return C.CString("Xray-core v" + core.Version())
}

// EngineFree frees a string the engine returned.
//
//export EngineFree
func EngineFree(s *C.char) {
	C.free(unsafe.Pointer(s))
}

func main() {}
