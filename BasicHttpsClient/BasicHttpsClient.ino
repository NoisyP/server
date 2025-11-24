/**
   BasicHTTPSClient.ino

    Created on: 14.10.2018

*/

#include <Arduino.h>

#include <WiFi.h>
#include <WiFiMulti.h>

#include <HTTPClient.h>

#include <NetworkClientSecure.h>

// This is a Google Trust Services cert, the root Certificate Authority that
// signed the server certificate for the demo server https://jigsaw.w3.org in this
// example. This certificate is valid until Jan 28 00:00:42 2028 GMT
const char *rootCACertificate = R"string_literal(
-----BEGIN CERTIFICATE-----
MIIDejCCAmKgAwIBAgIQf+UwvzMTQ77dghYQST2KGzANBgkqhkiG9w0BAQsFADBX
MQswCQYDVQQGEwJCRTEZMBcGA1UEChMQR2xvYmFsU2lnbiBudi1zYTEQMA4GA1UE
CxMHUm9vdCBDQTEbMBkGA1UEAxMSR2xvYmFsU2lnbiBSb290IENBMB4XDTIzMTEx
NTAzNDMyMVoXDTI4MDEyODAwMDA0MlowRzELMAkGA1UEBhMCVVMxIjAgBgNVBAoT
GUdvb2dsZSBUcnVzdCBTZXJ2aWNlcyBMTEMxFDASBgNVBAMTC0dUUyBSb290IFI0
MHYwEAYHKoZIzj0CAQYFK4EEACIDYgAE83Rzp2iLYK5DuDXFgTB7S0md+8Fhzube
Rr1r1WEYNa5A3XP3iZEwWus87oV8okB2O6nGuEfYKueSkWpz6bFyOZ8pn6KY019e
WIZlD6GEZQbR3IvJx3PIjGov5cSr0R2Ko4H/MIH8MA4GA1UdDwEB/wQEAwIBhjAd
BgNVHSUEFjAUBggrBgEFBQcDAQYIKwYBBQUHAwIwDwYDVR0TAQH/BAUwAwEB/zAd
BgNVHQ4EFgQUgEzW63T/STaj1dj8tT7FavCUHYwwHwYDVR0jBBgwFoAUYHtmGkUN
l8qJUC99BM00qP/8/UswNgYIKwYBBQUHAQEEKjAoMCYGCCsGAQUFBzAChhpodHRw
Oi8vaS5wa2kuZ29vZy9nc3IxLmNydDAtBgNVHR8EJjAkMCKgIKAehhxodHRwOi8v
Yy5wa2kuZ29vZy9yL2dzcjEuY3JsMBMGA1UdIAQMMAowCAYGZ4EMAQIBMA0GCSqG
SIb3DQEBCwUAA4IBAQAYQrsPBtYDh5bjP2OBDwmkoWhIDDkic574y04tfzHpn+cJ
odI2D4SseesQ6bDrarZ7C30ddLibZatoKiws3UL9xnELz4ct92vID24FfVbiI1hY
+SW6FoVHkNeWIP0GCbaM4C6uVdF5dTUsMVs/ZbzNnIdCp5Gxmx5ejvEau8otR/Cs
kGN+hr/W5GvT1tMBjgWKZ1i4//emhA1JG1BbPzoLJQvyEotc03lXjTaCzv8mEbep
8RqZ7a2CPsgRbuvTPBwcOMBBmuFeU88+FSBX6+7iP0il8b4Z0QFqIwwMHfs/L6K1
vepuoxtGzi4CZ68zJpiq1UvSqTbFJjtbD4seiMHl
-----END CERTIFICATE-----
)string_literal";

// OLED Display settings
#define SCREEN_WIDTH 128
#define SCREEN_HEIGHT 64
#define OLED_RESET -1
#define SCREEN_ADDRESS 0x3C
double loop_counter = 0;

enum STATUS {
  NONE,
  INITIALIZING,
  CONNECTING,
  SCANNING,
  SENDING
};

//INITIAL STATUS
STATUS status = STATUS::NONE;
// I2C pins for ESP32
#define SDA_PIN 21
#define SCL_PIN 22

// Create display object
Adafruit_SSD1306 display(SCREEN_WIDTH, SCREEN_HEIGHT, &Wire, OLED_RESET);

// Not sure if NetworkClientSecure checks the validity date of the certificate.
// Setting clock just to be sure...
void setClock() {
  configTime(0, 0, "pool.ntp.org");

  Serial.print(F("Waiting for NTP time sync: "));
  time_t nowSecs = time(nullptr);
  while (nowSecs < 8 * 3600 * 2) {
    delay(500);
    Serial.print(F("."));
    yield();
    nowSecs = time(nullptr);
  }

  Serial.println();
  struct tm timeinfo;
  gmtime_r(&nowSecs, &timeinfo);
  Serial.print(F("Current time: "));
  Serial.print(asctime(&timeinfo));
}

WiFiMulti WiFiMulti;

void setup() {
  status = STATUS::INITIALIZING;
  updateScreen();

  Serial.begin(115200);
  // Serial.setDebugOutput(true);
  //Setup screen 
  Wire.begin(SDA_PIN, SCL_PIN);
  
  // Initialize OLED display ///////////////////////////////
  if(!display.begin(SSD1306_SWITCHCAPVCC, SCREEN_ADDRESS)) {
    Serial.println(F("SSD1306 allocation failed"));
    for(;;); // Don't proceed, loop forever
  }
  
  Serial.println("OLED display initialized");
  //////////////////////////////////////////////////////////

  Serial.println();

  //Setting current status
  status = STATUS::CONNECTING;
  updateScreen();

  WiFi.mode(WIFI_STA);
  WiFiMulti.addAP("SSID", "PASSWORD");

  // wait for WiFi connection
  Serial.print("Waiting for WiFi to connect...");
  while ((WiFiMulti.run() != WL_CONNECTED)) {
    Serial.print(".");
  }
  Serial.println(" connected");
  
  setClock();
}

void updateScreen() {
    // Clear display and show startup message
  display.clearDisplay();
  display.setTextSize(1);
  display.setTextColor(SSD1306_WHITE);
  

  switch (status) {
  STATUS::CONNECTING: 
    display.setCursor(0, SCREEN_WIDTH - sizeOf(text));
    display.println("Connecting...");
    display.display();
    break;

  STATUS::SCANNING:
    display.setCursor(0, SCREEN_WIDTH - sizeOf(text));
    display.println("Scanning...");
    display.display();
    break;
  STATUS::SENDING:
    display.setCursor(0, SCREEN_WIDTH - sizeOf(text));
    display.println("Sending data to server...");
    display.display();
  }

  //Display how many wifi we are getting

}

void loop() {
  // NetworkClientSecure *client = new NetworkClientSecure;
  // if (loop) 
  // if (client) {
  //   client->setCACert(rootCACertificate);

  //   {
  //     // Add a scoping block for HTTPClient https to make sure it is destroyed before NetworkClientSecure *client is
  //     HTTPClient https;

  //     Serial.print("[HTTPS] begin...\n");
  //     if (https.begin(*client, "https://jigsaw.w3.org/HTTP/connection.html")) {  // HTTPS
  //       Serial.print("[HTTPS] GET...\n");
  //       // start connection and send HTTP header
  //       int httpCode = https.GET();

  //       // httpCode will be negative on error
  //       if (httpCode > 0) {
  //         // HTTP header has been send and Server response header has been handled
  //         Serial.printf("[HTTPS] GET... code: %d\n", httpCode);

  //         // file found at server
  //         if (httpCode == HTTP_CODE_OK || httpCode == HTTP_CODE_MOVED_PERMANENTLY) {
  //           String payload = https.getString();
  //           Serial.println(payload);
  //         }
  //       } else {
  //         Serial.printf("[HTTPS] GET... failed, error: %s\n", https.errorToString(httpCode).c_str());
  //       }

  //       https.end();
  //     } else {
  //       Serial.printf("[HTTPS] Unable to connect\n");
  //     }

  //     // End extra scoping block
  //   }

  //   delete client;
  // } else {
  //   Serial.println("Unable to create client");
  // }

  // Serial.println();
  // Serial.println("Waiting 10s before the next round...");
  // delay(10000);
}
