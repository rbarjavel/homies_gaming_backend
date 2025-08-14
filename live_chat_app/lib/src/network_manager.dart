import 'dart:io';

import 'package:http/http.dart' as http;
import 'package:path/path.dart';

class NetworkManager {
  static String url = 'http://70.0.0.118:3030/upload';
  static Future<bool> uploadImage(File imageFile) async {
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
}
