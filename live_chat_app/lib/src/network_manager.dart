import 'dart:io';

import 'package:http/http.dart' as http;
import 'package:path/path.dart';
import 'package:shared_preferences/shared_preferences.dart';

class NetworkManager {
  static String url = 'http://70.0.0.118:3030/upload';
  static String urlVideo = 'http://70.0.0.118:3030/upload-video';
  static Future<void> loadURLFromCache() async {
    final prefs = await SharedPreferences.getInstance();
    url = prefs.getString("url_server") ?? 'http://70.0.0.118:3030/upload';
  }

  static Future<void> setURLToCache(String url) async {
    final prefs = await SharedPreferences.getInstance();
    await prefs.setString("url_server", url);
  }

  static Future<bool> uploadImage(
    File imageFile, {
    String? text,
    int? duration,
  }) async {
    final uri = Uri.parse(url);
    final request = http.MultipartRequest('POST', uri);

    final stream = http.ByteStream(imageFile.openRead());
    final length = await imageFile.length();

    final multipartFile = http.MultipartFile(
      'image',
      stream,
      length,
      filename: basename(imageFile.path),
    );

    request.files.add(multipartFile);
    if (text != null) {
      request.fields["caption"] = text;
    }
    if (duration != null) {
      request.fields["duration"] = duration.toString();
    }

    try {
      final response = await request.send();

      if (response.statusCode == 200) {
        print('Image uploadée avec succès !');
        return true;
      } else {
        print(
          'Échec de l\'upload de l\'image avec le statut : ${response.statusCode}',
        );
        return false;
      }
    } catch (e) {
      print('Erreur lors de l\'upload de l\'image : $e');
      return false;
    }
  }

  static Future<bool> uploadVideoUrl({
    required String url,
    String? mesage,
  }) async {
    final uri = Uri.parse(urlVideo);
    final response = await http.post(
      uri,
      body: <String, String>{"video_url": url},
    );

    try {
      if (response.statusCode == 200) {
        print('Image uploadée avec succès !');
        return true;
      } else {
        print(
          'Échec de l\'upload de l\'image avec le statut : ${response.statusCode}',
        );
        return false;
      }
    } catch (e) {
      print('Erreur lors de l\'upload de l\'image : $e');
      return false;
    }
  }
}
