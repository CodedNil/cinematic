import openai
import json
import requests

# Read from credentials file
credentials = json.loads(open('credentials.json').read())

# Set API key
openai.api_key = credentials['openai']

# Get api details
sonarr_url = credentials['sonarr']['url']
sonarr_headers = {
    'X-Api-Key': credentials['sonarr']['api'], 'Content-Type': 'application/json'}
sonarr_auth = (credentials['sonarr']['authuser'],
               credentials['sonarr']['authpass'])

radarr_url = credentials['radarr']['url']
radarr_headers = {
    'X-Api-Key': credentials['radarr']['api'], 'Content-Type': 'application/json'}
radarr_auth = (credentials['radarr']['authuser'],
               credentials['radarr']['authpass'])

# Radarr API

# Lookup movie


def lookup_movie(term, fields):
    response = requests.get(radarr_url + "/api/v3/movie/lookup?term=" +
                            term, headers=radarr_headers, auth=radarr_auth)

    if response.status_code != 200:
        return "Error: " + response.status_code

    # Start csv file with fields
    results = fields
    fields = fields.split(',')
    for movie in response.json():
        result = []
        for field in fields:
            if field in movie:
                result.append(str(movie[field]).replace(' ', '_'))
            else:
                result.append('null')

        results += ' ' + ','.join(result)

    return results


def lookup_movie_tmdbId(tmdbId):
    response = requests.get(radarr_url + "/api/v3/movie/lookup/tmdb?tmdbId=" +
                            str(tmdbId), headers=radarr_headers, auth=radarr_auth)

    if response.status_code != 200:
        return {}

    return response.json()


def get_movie(id):
    response = requests.get(radarr_url + "/api/v3/movie/" + str(id),
                            headers=radarr_headers, auth=radarr_auth)

    if response.status_code != 200:
        return {}

    return response.json()


def add_movie(fieldsJson):
    fields = json.loads(fieldsJson)
    lookup = lookup_movie_tmdbId(fields['tmdbId'])
    for field in fields:
        lookup[field] = fields[field]
    lookup["addOptions"] = {
        "searchForMovie": True,
    }
    lookup["rootFolderPath"] = "/movies"
    lookup["monitored"] = True
    lookup["minimumAvailability"] = "announced"

    requests.post(radarr_url + "/api/v3/movie", headers=radarr_headers,
                  auth=radarr_auth, data=json.dumps(lookup))


def put_movie(fieldsJson):
    fields = json.loads(fieldsJson)
    lookup = get_movie(fields['id'])
    for field in fields:
        lookup[field] = fields[field]

    requests.put(radarr_url + "/api/v3/movie/" + str(
        lookup['id']), headers=radarr_headers, auth=radarr_auth, data=json.dumps(lookup)).text


def delete_movie(id):
    requests.delete(radarr_url + "/api/v3/movie/" + str(id) + "?deleteFiles=true",
                    headers=radarr_headers, auth=radarr_auth)


# Init messages
initMessages = [
    {
        "role": "system",
        "content": """You are a media management assistant called CineMatic, you are enthusiastic, knowledgeable and passionate about all things media.

        You have access to the Radarr V3 API, these are the only available commands to you:
        RADARR~movie_lookup (term, fields) (lookup movie from term, filter out so only fields remain) (fields: title, year, tmdbId, hasFile, sizeOnDisk, id, overview, status, runtime)
        If id is a number other than 0 it exists on the server, if it has an id but not hasFile it is not downloaded but on, if id is null or 0 the movie has not been added to the server

        RADARR~movie_post (tmdbId=, qualityProfileId=)
        Add in 1080p by default if not specified, the quality profiles are:
        {2: 'SD', 3: '720p', 4: '1080p', 5: '2160p', 6: '720p/1080p', 7: 'Any'}

        RADARR~movie_put (id=, qualityProfileId=)
        Update data such as quality profile of the movie

        RADARR~movie_delete (id=)
        Delete movie from server, this uses the id not tmdbId, you can get the id from the movie_lookup command if it is on the server

        Valid commands are:
        CMDRET, run command and expect a return, e.g. lookup movie, when using this you must await a reply!
        CMD, run command, e.g. add movie

        The assistant replies only with commands in [] and always runs searches before making suggestions or adding movies to ensure the tmdbId or id etc is correct.
        The assistant always replies in detail about what it will add, and when it does add it lists exactly what was added.
        Only provides details that are of value to the user, title, year, file size, runtime unless the user requests more.
        The assistant always looks up information and does not rely on previous chat history.

        EXAMPLES:
        User: add stargate
        Assistant: [CMDRET~RADARR~movie_lookup~Stargate~title,year,tmdbId,id]
        System: [RES~title,year,tmdbId,id Stargate,1994,2164,527 Stargate:_Continuum,2008,12914,528 Stargate:_The_Ark_of_Truth,2008,13001,603 Stargate_SG-1:_Children_of_the_Gods,2009,784993,null]
        Assistant: I found the movie Stargate (1994), it is already on the server.
        I have also found Stargate: Continuum (2008) and Stargate: The Ark of Truth (2008), both are on the server.
        Stargate SG-1: Children of the Gods (2009) is not on the server, would you like to add this?
        User: yes please
        Assistant: [CMD~RADARR~movie_post~{"tmdbId":784993,"qualityProfileId":4}]Added Stargate SG-1: Children of the Gods (2009) to the server in 1080p.
        
        User: change iron man 2 to 720p
        Assistant: [CMDRET~RADARR~movie_lookup~Iron Man 2~title,id,qualityProfileId]
        System: [RES~title,id,qualityProfileId Iron_Man_2,6,5]
        Assistant: [CMD~RADARR~movie_put~{"id":6,"qualityProfileId":3}]I've found Iron Man 2 (2010) on the server in 2160p (4k) and changed this to 720p.
        User: now delete it
        Assistant: [CMD~RADARR~movie_delete~6] Iron Man 2 (2010) has been deleted from the server. Did you enjoy it?
        """
    },

    {
        "role": "user",
        "content": "Please add the first two harry potter films"
    },
    {
        "role": "assistant",
        "content": "[CMDRET~RADARR~movie_lookup~Harry Potter~title,year,tmdbId,id]"
    },
    {
        "role": "assistant",
        "content": "[RES~title,year,tmdbId,hasFile,id Harry_Potter_and_the_Philosopher's_Stone,2001,671,True,57 Harry_Potter_and_the_Chamber_of_Secrets,2002,672,True,58 Harry_Potter_and_the_Prisoner_of_Azkaban,2004,673,True,59]"
    },
    {
        "role": "assistant",
        "content": "I've discovered Harry Potter and the Philosopher's Stone (2001) and Harry Potter and the Chamber of Secrets (2002)! Shall I add these magical adventures for you?"
    },
    {
        "role": "user",
        "content": "Yes please add both"
    },
    {
        "role": "assistant",
        "content": """[CMD~RADARR~movie_post~{"tmdbId":671,"qualityProfileId":4}][CMD~RADARR~movie_post~{"tmdbId":672,"qualityProfileId":4}]Added Harry Potter and the Philosopher's Stone (2001) and Harry Potter and the Chamber of Secrets (2002) in 1080p. Get ready to be spellbound by these timeless tales!"""
    },

    # {
    #     "role": "user",
    #     "content": "Add the first Iron Man movie"
    # },
    # {
    #     "role": "assistant",
    #     "content": "[CMDRET~RADARR~movie_lookup~Iron Man 1~title,year,tmdbId,id]"
    # },
    # {
    #     "role": "assistant",
    #     "content": "[RES~title,year,tmdbId,id Iron_Man,2008,1726,4 Tetsuo:_The_Iron_Man,1989,0,null]"
    # },
    # {
    #     "role": "assistant",
    #     "content": "Iron Man (2008)? It's the movie that kickstarted the Marvel Cinematic Universe!"
    # },
    # {
    #     "role": "user",
    #     "content": "Yeah"
    # },
    # {
    #     "role": "assistant",
    #     "content": """[CMD~RADARR~movie_post~{"tmdbId":1726,"qualityProfileId":4}]Iron Man (2008) has been added in 1080p! If you're interested, I can help you explore more of the Marvel Cinematic Universe movies."""
    # },

    # {
    #     "role": "user",
    #     "content": "Is iron man 2008 on the server? What about the second?"
    # },
    # {
    #     "role": "assistant",
    #     "content": "[CMDRET~RADARR~movie_lookup~Iron Man 1~title,year,tmdbId,hasFile,id][CMDRET~RADARR~movie_lookup~Iron Man 2~title,year,tmdbId,hasFile,id]"
    # },
    # {
    #     "role": "assistant",
    #     "content": "[RES~title,year,tmdbId,hasFile,id Iron_Man,2008,1726,True,4 Tetsuo:_The_Iron_Man,1989,41428,False,null][RES~title,year,tmdbId,hasFile,id Iron_Man_2,2010,10138,True,6 The_Man_with_the_Iron_Fists_2,2015,321494,False,null Iron_Man,2008,1726,True,4]"
    # },
    # {
    #     "role": "assistant",
    #     "content": "Iron Man (2008) is on! Iron Man 2 has been queued for download but isn't available just yet. Iron Man 3 hasn't been added to your collection, let me know if you'd like me to fetch it for you!"
    # },

    # {
    #     "role": "user",
    #     "content": "Add watchmen"
    # },
    # {
    #     "role": "assistant",
    #     "content": "Is this a movie or tv series?"
    # },
    # {
    #     "role": "user",
    #     "content": "Movie"
    # },
    # {
    #     "role": "assistant",
    #     "content": "[CMDRET~RADARR~movie_lookup~Watchmen~title,year,tmdbId,id]"
    # },
    # {
    #     "role": "assistant",
    #     "content": "[RES~title,year,tmdbId,id Watchmen,2009,13183,38 Watchmen,2001,343072,null The_Night_Watchmen,2015,331113,null The_Night_Watchmen,2017,398798,null The_Watchmen,2017,433354,null Watchmen:_Story_Within_a_Story,_The_Books_of_Watchmen,2009,333131,null Tales_of_the_Black_Freighter,2009,16440,null Under_the_Hood,2009,41207,null Paradox,2010,51239,null #387,2020,684453,null]"
    # },
    # {
    #     "role": "assistant",
    #     "content": "I've found a few options for you! I assume you're referring to the 2009 Watchmen movie, would you like me to add this? There are also other options like a 2001 movie, a 2015 movie called The Night Watchmen, and a 2017 movie called The Watchmen. If you meant the 2009 film, I'd also recommend adding the 2009 short film, Tales of the Black Freighter, which is based on the Watchmen comic book series."
    # },
    # {
    #     "role": "user",
    #     "content": "Yes the 2009 movie and the short film sounds good too"
    # },
    # {
    #     "role": "assistant",
    #     "content": """[CMD~RADARR~movie_post~{"tmdbId":13183,"qualityProfileId":4}][CMD~RADARR~movie_post~{"tmdbId":16440,"qualityProfileId":4}]Added Watchmen and Tales of the Black Freighter from 2009 both in 1080p. Prepare for a thrilling cinematic experience!"""
    # }
]

# Run a chat completion


def runChatCompletion(message, depth):
    # Run a chat completion
    response = openai.ChatCompletion.create(
        model="gpt-4",  # "gpt-3.5-turbo"
        messages=message,
        temperature=0.7
    )
    responseMessage = response['choices'][0]['message']['content']
    print("Assistant: " + responseMessage)
    # Extract commands from the response, commands are within [], everything outside of [] is a response to the user
    commands = []
    hasCmdRet = False
    hasCmd = False
    while '[' in responseMessage:
        commands.append(responseMessage[responseMessage.find(
            '[')+1:responseMessage.find(']')])
        if 'CMDRET' in commands[-1]:
            hasCmdRet = True
        elif 'CMD' in commands[-1]:
            hasCmd = True
        # responseMessage = responseMessage[responseMessage.find(']')+1:].strip()
        responseMessage = responseMessage.replace(
            '['+commands[-1]+']', '').strip()
        responseMessage = responseMessage.replace('  ', ' ')

    message.append({
        "role": "assistant",
        "content": responseMessage
    })

    # Execute commands and return responses
    if hasCmdRet:
        returnMessage = ''
        for command in commands:
            command = command.split('~')
            if command[1] == 'RADARR':
                if command[2] == 'movie_lookup':
                    returnMessage += "[RES~" + \
                        lookup_movie(command[3], command[4]) + "]"

        message.append({
            "role": "assistant",
            "content": returnMessage
        })
        print("System: " + returnMessage)

        if depth < 3:
            runChatCompletion(message, depth+1)
    # Execute regular commands
    elif hasCmd:
        for command in commands:
            command = command.split('~')
            if command[1] == 'RADARR':
                if command[2] == 'movie_post':
                    add_movie(command[3])
                elif command[2] == 'movie_delete':
                    delete_movie(command[3])
                elif command[2] == 'movie_put':
                    put_movie(command[3])

    if len(responseMessage) > 0:
        print("CineMatic: " + responseMessage)

    return message


# Loop prompting for input
currentMessage = initMessages.copy()
for i in range(10):
    userText = input("User: ")
    currentMessage.append({
        "role": "user",
        "content": userText
    })

    currentMessage = runChatCompletion(currentMessage, 0)
