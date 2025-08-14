package main

import (
	"live_chat/src/constant"
	"live_chat/src/setup"
	"live_chat/src/websocket"
	"os"
	"runtime"
)

func main() {
	var wsURL string
	if len(os.Args) < 2 {
		wsURL = "ws://" + constant.IP_ADDR_SERVER + "/ws"
	} else {
		wsURL = "ws://" + os.Args[1] + "/ws"
		constant.IP_ADDR_SERVER = os.Args[1]
	}

	if runtime.GOOS == "windows" {
		setup.SetupStartup()
		setup.SetupFirewall(constant.PORT_ADDR)
	}

	for {
		websocket.ConnectToWebsocket(wsURL)
	}
}
