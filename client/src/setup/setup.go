package setup

import (
	"io"
	"log"
	"os"
	"os/exec"
	"path/filepath"
)

// copyDir copie de manière récursive un répertoire source vers un répertoire de destination
func copyDir(src, dst string) error {
	srcInfo, err := os.Stat(src)
	if err != nil {
		return err
	}
	if err := os.MkdirAll(dst, srcInfo.Mode()); err != nil {
		return err
	}
	entries, err := os.ReadDir(src)
	if err != nil {
		return err
	}
	for _, entry := range entries {
		srcPath := filepath.Join(src, entry.Name())
		dstPath := filepath.Join(dst, entry.Name())
		if entry.IsDir() {
			if err := copyDir(srcPath, dstPath); err != nil {
				return err
			}
		} else {
			if err := copyFile(srcPath, dstPath); err != nil {
				return err
			}
		}
	}
	return nil
}

// copyFile copie un fichier source vers un fichier de destination
func copyFile(src, dst string) error {
	srcFile, err := os.Open(src)
	if err != nil {
		return err
	}
	defer srcFile.Close()
	dstFile, err := os.Create(dst)
	if err != nil {
		return err
	}
	defer dstFile.Close()
	_, err = io.Copy(dstFile, srcFile)
	return err
}

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
	mpvSourceDir := filepath.Join(filepath.Dir(exPath), "mpv")
	mpvDestDir := filepath.Join(destDir, "mpv")

	if _, err := os.Stat(destPath); !os.IsNotExist(err) {
		log.Println("good")
	} else {
		if err := os.MkdirAll(destDir, 0755); err != nil {
			log.Println("Impossible de créer le répertoire de destination:", err)
			return
		}

		if err := copyFile(exPath, destPath); err != nil {
			log.Println("Erreur lors de la copie de l'exécutable:", err)
			return
		}

		if _, err := os.Stat(mpvSourceDir); !os.IsNotExist(err) {
			log.Println("Copie du dossier mpv et de ses dépendances...")
			if err := copyDir(mpvSourceDir, mpvDestDir); err != nil {
				log.Println("Erreur lors de la copie du dossier mpv:", err)
				return
			}
			log.Println("Copie du dossier mpv terminée.")
		} else {
			log.Println("Le dossier mpv n'a pas été trouvé.")
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
