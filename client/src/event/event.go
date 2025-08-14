package event

import (
	"fmt"
	"live_chat/src/constant"
	"log"
	"net/http"
	"os/exec"
	"runtime"
	"time"

	"github.com/ebitengine/oto/v3"
	"github.com/hajimehoshi/go-mp3"
)

func DispatchEvent(json map[string]string) {
	switch json["event"] {
	case "browser_backend":
		if _, ok := json["url"]; ok {
			openBrowser("http://" + constant.IP_ADDR_SERVER + json["url"])
		} else {
			log.Println("no url found")
		}
	case "song":
		if _, ok := json["url"]; ok {
			playSong("http://" + constant.IP_ADDR_SERVER + json["url"])
		} else {
			log.Println("no url found")
		}
	case "browser_raw":
		if _, ok := json["url"]; ok {
			openBrowser(json["url"])
		} else {
			log.Println("no url found")
		}
	default:
		log.Println("default:", json)
	}
}

func openBrowser(url string) {
	var cmd *exec.Cmd
	switch runtime.GOOS {
	case "windows":
		cmd = exec.Command("cmd", "/c", "start", url)
	case "darwin":
		cmd = exec.Command("open", url)
	default:
		cmd = exec.Command("xdg-open", url)
	}

	err := cmd.Run()
	if err != nil {
		log.Println("Impossible d'ouvrir le navigateur:", err)
	}
}

func playSong(url string) {
	fmt.Println("Downloading sound from:", url)

	// Télécharger le fichier audio depuis l'URL
	resp, err := http.Get(url)
	if err != nil {
		log.Fatal(err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		log.Fatalf("bad status: %s", resp.Status)
	}

	decodedMp3, err := mp3.NewDecoder(resp.Body)
	if err != nil {
		panic("mp3.NewDecoder failed: " + err.Error())
	}

	op := &oto.NewContextOptions{}
	// Usually 44100 or 48000. Other values might cause distortions in Oto
	op.SampleRate = 44100
	// Number of channels (aka locations) to play sounds from. Either 1 or 2.
	// 1 is mono sound, and 2 is stereo (most speakers are stereo).
	op.ChannelCount = 2
	// Format of the source. go-mp3's format is signed 16bit integers.
	op.Format = oto.FormatSignedInt16LE

	otoCtx, readyChan, err := oto.NewContext(op)
	if err != nil {
		panic("oto.NewContext failed: " + err.Error())
	}

	<-readyChan

	player := otoCtx.NewPlayer(decodedMp3)

	player.Play()

	for player.IsPlaying() {
		time.Sleep(time.Millisecond)
	}

	err = player.Close()
	if err != nil {
		panic("player.Close failed: " + err.Error())
	}
}
