import 'dart:async';
import 'dart:io';

import 'package:flutter/material.dart';
import 'package:flutter/services.dart';
import 'package:flutter_exit_app/flutter_exit_app.dart';
import 'package:image_picker/image_picker.dart';
import 'package:live_chat_app/src/network_manager.dart';
import 'package:numberpicker/numberpicker.dart';
import 'package:receive_sharing_intent/receive_sharing_intent.dart';
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
  final TextEditingController controllerText = TextEditingController();
  final FocusNode textFocus = FocusNode();
  bool isEditFocus = false;
  int durationValue = 5;

  late ShakeDetector detector;

  bool bottomSheetOpen = false;

  @override
  void dispose() {
    controllerText.dispose();
    textFocus.dispose();
    super.dispose();
  }

  late StreamSubscription _intentSub;
  final _sharedFiles = <SharedMediaFile>[];

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

    // Listen to media sharing coming from outside the app while the app is in the memory.
    _intentSub = ReceiveSharingIntent.instance.getMediaStream().listen(
      (value) {
        setState(() {
          _sharedFiles.clear();
          _sharedFiles.addAll(value);

          print("shared files app open: ${_sharedFiles.map((f) => f.toMap())}");
          for (SharedMediaFile file in _sharedFiles) {
            NetworkManager.uploadVideoUrl(
              url: file.path,
              mesage: file.message,
            ).then((bool res) {
              if (res) {
                FlutterExitApp.exitApp();
              }
            });
          }
        });
      },
      onError: (err) {
        print("getIntentDataStream error: $err");
      },
    );

    // Get the media sharing coming from outside the app while the app is closed.
    ReceiveSharingIntent.instance.getInitialMedia().then((value) {
      setState(() {
        _sharedFiles.clear();
        _sharedFiles.addAll(value);
        print(
          "shared files when app closed: ${_sharedFiles.map((f) => f.toMap())}",
        );
        for (SharedMediaFile file in _sharedFiles) {
          NetworkManager.uploadVideoUrl(
            url: file.path,
            mesage: file.message,
          ).then((bool res) {
            if (res) {
              FlutterExitApp.exitApp();
            }
          });
        }
        // Tell the library that we are done processing the intent.
        ReceiveSharingIntent.instance.reset();
      });
    });
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
    Size size = MediaQuery.of(context).size;
    return PopScope(
      canPop: false,
      child: Scaffold(
        backgroundColor: Colors.black38,
        body: SafeArea(
          child: Padding(
            padding: const EdgeInsets.only(
              top: 32.0,
              bottom: 24,
              left: 16,
              right: 16,
            ),
            child: Column(
              children: <Widget>[
                Expanded(
                  flex: 1,
                  child: const Center(
                    child: Text(
                      'LIVE CHAT UPLOADER',
                      style: TextStyle(
                        fontSize: 28,
                        fontWeight: FontWeight.bold,
                        color: Colors.white,
                      ),
                    ),
                  ),
                ),
                if (_image == null)
                  const Center(
                    child: Text(
                      'Aucune image sélectionnée.',
                      style: TextStyle(color: Colors.white),
                    ),
                  )
                else
                  Expanded(
                    flex: 4,
                    child: Center(
                      child: GestureDetector(
                        onTap: () {
                          if (!isEditFocus) {
                            FocusScope.of(context).requestFocus(textFocus);
                            isEditFocus = true;
                          } else {
                            isEditFocus = false;
                          }
                        },
                        child: Container(
                          decoration: BoxDecoration(
                            image: DecorationImage(
                              fit: BoxFit.contain,
                              image: Image.file(_image!).image,
                            ),
                          ),
                        ),
                      ),
                    ),
                  ),
                if (_image != null)
                  Expanded(
                    flex: 1,
                    child: Container(
                      margin: EdgeInsets.symmetric(horizontal: 16),
                      child: Center(
                        child: TextField(
                          textAlign: TextAlign.center,
                          decoration: InputDecoration(
                            focusedBorder: InputBorder.none,
                            enabledBorder: InputBorder.none,
                            border: InputBorder.none,
                          ),
                          controller: controllerText,
                          focusNode: textFocus,
                          onTapOutside: (_) {
                            FocusManager.instance.primaryFocus?.unfocus();
                          },
                          cursorColor: Colors.white,
                          maxLines: null,
                          style: TextStyle(
                            color: Colors.white,
                            fontSize: 16,
                            fontWeight: FontWeight.bold,
                          ),
                        ),
                      ),
                    ),
                  ),
                if (_image != null)
                  Expanded(
                    flex: 1,
                    child: NumberPicker(
                      value: durationValue,
                      minValue: 5,
                      haptics: true,
                      maxValue: 60,
                      axis: Axis.horizontal,
                      textStyle: TextStyle(color: Colors.white),
                      selectedTextStyle: TextStyle(
                        color: Colors.white,
                        fontSize: 24,
                        fontWeight: FontWeight.bold,
                      ),
                      onChanged: (value) =>
                          setState(() => durationValue = value),
                    ),
                  ),
                if (_image != null)
                  ElevatedButton(
                    style: ButtonStyle(
                      backgroundColor: WidgetStateProperty.all(Colors.amber),
                    ),
                    onPressed: () async {
                      if (_image != null) {
                        bool success = await NetworkManager.uploadImage(
                          _image!,
                          text: controllerText.text.isNotEmpty
                              ? controllerText.text
                              : null,
                          duration: durationValue,
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
      ),
    );
  }
}
