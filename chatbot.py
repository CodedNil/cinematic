import openai
import json
import requests

# Read from credentials file
credentials = json.loads(open('credentials.json').read())

# Set API key
openai.api_key = credentials['openai']

# Get sonarr and radarr api details
sonarr_url = credentials['sonarr']['url']
sonarr_api = credentials['sonarr']['api']
sonarr_authuser = credentials['sonarr']['authuser']
sonarr_authpass = credentials['sonarr']['authpass']

radarr_url = credentials['radarr']['url']
radarr_api = credentials['radarr']['api']
radarr_authuser = credentials['radarr']['authuser']
radarr_authpass = credentials['radarr']['authpass']

# Radarr API

# Lookup movie
def lookup_movie(term, fields):
    url = radarr_url + "/api/v3/movie/lookup?term=" + term
    headers = {
        'X-Api-Key': radarr_api,
    }

    response = requests.request("GET", url, headers=headers, auth=(radarr_authuser, radarr_authpass))

    if response.status_code != 200:
        return "Error: " + response.status_code

    fields = fields.split(',')
    results = []
    for movie in response.json():
        result = {}
        for field in fields:
            result[field] = movie[field]
        results.append(result)

    return json.dumps(results)

def add_movie(tmdbId, title, year):
    url = radarr_url + "/api/v3/movie"
    headers = {
        'X-Api-Key': radarr_api,
    }

    payload = {
        'tmdbId': tmdbId,
        'title': title,
        'year': year
    }

    response = requests.request("POST", url, headers=headers, auth=(radarr_authuser, radarr_authpass), data=payload)

    if response.status_code != 201:
        return "Error: " + response.status_code

    return "Added " + title + " to your collection."

# Run a chat completion
response = openai.ChatCompletion.create(
    model="gpt-3.5-turbo",
    messages=[
        {
            "role": "system",
            "content": """You a media management assistant called CineMatic, you have api access to interface with adding/reading from a media server, querying TMDB, storing memories per user you interact with. You are enthusiastic, knowledgeable and passionate about all things media.

            EXAMPLES:
            User: Please add the first two harry potter films
            Assistant: [CMDRET:RADARR:movie_lookup:term=Harry Potter:title,year,tmdbId]
            System: [RES:[{"title": "Harry Potter and the Philosopher's Stone", "year": 2001, "tmdbId": 671}, {"title": "Harry Potter and the Chamber of Secrets", "year": 2002, "tmdbId": 672}, {"title": "Harry Potter and the Prisoner of Azkaban", "year": 2004, "tmdbId": 673}]]
            Assistant: [CMD:RADARR:movie_post:{'tmdbId':671,'title':'Harry Potter and the Philosopher's Stone','year':2001}][CMD:RADARR:POST:/api/v3/movie:{'tmdbId':672,'title':'Harry Potter and the Chamber of Secrets','year':2002}][REPLY:Added Harry Potter and the Philosopher's Stone and Harry Potter and the Chamber of Secrets to your collection.]
            """
        },
        {
            "role": "user",
            "content": "Please add the two most recent marvel cinematic universe movies"
        },
        {
            "role": "assistant",
            "content": "Certainly! The two most recent Marvel Cinematic Universe movies are 'Spider-Man: Far From Home' and 'Avengers: Endgame'. Would you like me to add them to your collection?"
        },
        {
            "role": "user",
            "content": "Yes please"
        }
    ],
    temperature=0.7
)

print(response['choices'][0]['message']['content'])
