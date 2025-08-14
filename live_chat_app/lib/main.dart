import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:image_picker/image_picker.dart';
import 'package:live_chat_app/src/network_manager.dart';
import 'package:shake/shake.dart';

void main() async {
  WidgetsFlutterBinding.ensureInitialized();
  await SystemChrome.setPreferredOrientations([
    DeviceOrientation.portraitUp,
    DeviceOrientation.portraitDown,
  ]);

  await NetworkManager.loadURLFromCache();
  runApp(const MyApp());
}

class MyApp extends StatelessWidget {
  const MyApp({super.key});
  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'Flutter Demo',
      debugShowCheckedModeBanner: false,
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: Colors.deepPurple),
      ),
      home: const MyHomePage(title: 'Flutter Demo Home Page'),
    );
  }
}

class MyHomePage extends StatefulWidget {
  const MyHomePage({super.key, required this.title});

  final String title;

  @override
  State<MyHomePage> createState() => _MyHomePageState();
}

class _MyHomePageState extends State<MyHomePage> {
  File? _image;
  final ImagePicker _picker = ImagePicker();

  late ShakeDetector detector;

  bool bottomSheetOpen = false;

  @override
  void initState() {
    super.initState();
    detector = ShakeDetector.autoStart(
      onPhoneShake: (ShakeEvent event) async {
        if (!bottomSheetOpen) {
          bottomSheetOpen = true;
          showModalSettings();
          bottomSheetOpen = false;
        }
      },
      shakeThresholdGravity: 3,
    );
  }

  void showModalSettings() async {
    await showModalBottomSheet(
      context: context,
      isScrollControlled: true, // Ajoutez cette ligne
      builder: (BuildContext context) {
        Size size = MediaQuery.of(context).size;
        TextEditingController controller = TextEditingController(
          text: NetworkManager.url,
        );
        return Padding(
          padding: EdgeInsets.only(
            bottom: MediaQuery.of(context).viewInsets.bottom,
            left: 16.0,
            right: 16.0,
            top: 16.0,
          ),
          child: SizedBox(
            height: size.height * 0.2,
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                TextField(
                  controller: controller,
                  decoration: const InputDecoration(
                    labelText: 'Nouvelle adresse',
                    hintText: 'Entrez une nouvelle adresse',
                    filled: true,
                    border: OutlineInputBorder(
                      borderRadius: BorderRadius.all(Radius.circular(12.0)),
                    ),
                  ),
                  onChanged: (text) {
                    NetworkManager.url = text;
                  },
                ),
              ],
            ),
          ),
        );
      },
    );
    await NetworkManager.setURLToCache(NetworkManager.url);
  }

  Future<void> _pickImage() async {
    final XFile? image = await _picker.pickImage(source: ImageSource.gallery);
    if (image != null) {
      setState(() {
        _image = File(image.path);
      });
    }
  }

  @override
  Widget build(BuildContext context) {
    return Scaffold(
      backgroundColor: Colors.black38,
      body: SafeArea(
        child: Padding(
          padding: const EdgeInsets.all(16.0),
          child: Column(
            children: <Widget>[
              const Center(
                child: Text(
                  'LIVE CHAT UPLOADER',
                  style: TextStyle(
                    fontSize: 24,
                    fontWeight: FontWeight.bold,
                    color: Colors.white,
                  ),
                ),
              ),
              const SizedBox(height: 20),
              if (_image == null)
                const Text(
                  'Aucune image sélectionnée.',
                  style: TextStyle(color: Colors.white),
                )
              else
                Expanded(
                  child: Center(
                    child: Container(
                      decoration: BoxDecoration(
                        borderRadius: BorderRadius.all(Radius.circular(14)),
                        image: DecorationImage(
                          image: Image.file(_image!).image,
                        ),
                      ),
                    ),
                  ),
                ),
              if (_image != null)
                Padding(
                  padding: const EdgeInsets.only(top: 20.0),
                  child: ElevatedButton(
                    style: ButtonStyle(
                      backgroundColor: WidgetStateProperty.all(Colors.amber),
                    ),
                    onPressed: () async {
                      if (_image != null) {
                        bool success = await NetworkManager.uploadImage(
                          _image!,
                        );
                        if (success) {
                          print('Upload réussi !');
                        } else {
                          print('Upload a échoué.');
                        }
                      }
                    },
                    child: const Text(
                      "Upload",
                      style: TextStyle(color: Colors.black, fontSize: 16),
                    ),
                  ),
                ),
            ],
          ),
        ),
      ),
      floatingActionButton: FloatingActionButton(
        backgroundColor: Colors.amber,
        onPressed: _pickImage,
        tooltip: 'Upload image',
        child: const Icon(Icons.add),
      ),
    );
  }
}
