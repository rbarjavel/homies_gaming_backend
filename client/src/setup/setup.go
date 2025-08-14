package setup

import (
	"io"
	"log"
	"os"
	"os/exec"
	"path/filepath"
)

func SetupStartup() {
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

func SetupFirewall(port string) {
	log.Println("Configuration du pare-feu Windows pour autoriser le port +" + port + "+...")
	cmd := exec.Command("netsh", "advfirewall", "firewall", "add", "rule",
		"name=live_chat", "dir=in", "action=allow", "protocol=TCP", "localport="+port)

	output, err := cmd.CombinedOutput()
	if err != nil {
		log.Printf("Erreur lors de la configuration du pare-feu: %s\n", err)
		log.Printf("Sortie de la commande: %s\n", output)
		return
	}
	log.Println("Règle de pare-feu pour le port 3030 ajoutée avec succès.")
}
