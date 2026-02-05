#!/usr/bin/env python3
import urllib.request
import json

# Boston coordinates
LAT = 42.3601
LON = -71.0589

# Open-Meteo API (free, no API key required)
url = f'https://api.open-meteo.com/v1/forecast?latitude={LAT}&longitude={LON}&current=temperature_2m,relative_humidity_2m,apparent_temperature,precipitation,weather_code,wind_speed_10m&temperature_unit=fahrenheit&wind_speed_unit=mph'

try:
    with urllib.request.urlopen(url, timeout=10) as response:
        data = json.loads(response.read().decode())
        current = data['current']
        
        # Weather code descriptions
        weather_codes = {
            0: 'Clear sky',
            1: 'Mainly clear', 2: 'Partly cloudy', 3: 'Overcast',
            45: 'Foggy', 48: 'Depositing rime fog',
            51: 'Light drizzle', 53: 'Moderate drizzle', 55: 'Dense drizzle',
            61: 'Slight rain', 63: 'Moderate rain', 65: 'Heavy rain',
            71: 'Slight snow', 73: 'Moderate snow', 75: 'Heavy snow',
            80: 'Slight rain showers', 81: 'Moderate rain showers', 82: 'Violent rain showers',
            95: 'Thunderstorm', 96: 'Thunderstorm with slight hail', 99: 'Thunderstorm with heavy hail'
        }
        
        weather_desc = weather_codes.get(current['weather_code'], 'Unknown')
        
        print('=' * 50)
        print('🌤️  Current Weather in Boston, MA')
        print('=' * 50)
        print(f"Conditions: {weather_desc}")
        print(f"Temperature: {current['temperature_2m']}°F")
        print(f"Feels Like: {current['apparent_temperature']}°F")
        print(f"Humidity: {current['relative_humidity_2m']}%")
        print(f"Wind Speed: {current['wind_speed_10m']} mph")
        print(f"Precipitation: {current['precipitation']} mm")
        print('=' * 50)
        
except Exception as e:
    print(f'Error fetching weather: {e}')
