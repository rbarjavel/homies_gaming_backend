package websocket

import (
	"encoding/json"
	"live_chat/src/event"
	"log"
	"net/url"
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
		go event.DispatchEvent(messageJSON)
	}
}
