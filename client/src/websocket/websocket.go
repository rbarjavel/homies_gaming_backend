package websocket

import (
	"encoding/json"
	"live_chat/src/constant"
	"log"
	"net/url"
	"os/exec"
	"runtime"
	"time"

	"github.com/gorilla/websocket"
)

var ClientConn *websocket.Conn

func ConnectToWebsocket(wsURL string) {
	u, err := url.Parse(wsURL)
	if err != nil {
		log.Fatal("URL WebSocket invalide:", err)
	}

	c, _, err := websocket.DefaultDialer.Dial(u.String(), nil)
	if err != nil {
		log.Println("Échec de la connexion au serveur WebSocket:", err)
		time.Sleep(time.Second)
		return
	}

	ClientConn = c
	log.Printf("Connecté au serveur WebSocket: %s", u.String())

	// Écoute des messages du serveur en arrière-plan
	defer ClientConn.Close()
	for {
		typeM, message, err := ClientConn.ReadMessage()
		if err != nil {
			log.Println("Erreur de lecture WebSocket:", err)
			return
		}
		log.Printf("Message reçu du serveur distant: %s, %d", message, typeM)
		var messageJSON map[string]string
		err = json.Unmarshal(message, &messageJSON)
		if err != nil {
			log.Println("error:", err)
		}
		if _, ok := messageJSON["url"]; ok {
			openBrowser("http://" + constant.IP_ADDR_SERVER + messageJSON["url"])
		} else {
			log.Println("no url found")
		}
	}
}

func openBrowser(url string) {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "windows":
		cmd = exec.Command("cmd", "/c", "start", url)
	case "darwin": // macOS
		cmd = exec.Command("open", url)
	default: // linux, bsd, etc.
		cmd = exec.Command("xdg-open", url)
	}

	err := cmd.Run()
	if err != nil {
		log.Println("Impossible d'ouvrir le navigateur:", err)
	}
}
