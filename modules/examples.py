import openai
import modules.module_logs as ModuleLogs

Examples = [
    # API - Resolution api
    {
        "queries": [
            "wants resolution queried",
            "wants movie added",
            "wants series added",
            "wants resolution changed",
        ],
        "prompt": "U:Assistant adds in 1080p (qualityProfileId 4) by default, the quality profiles are: 2=SD 3=720p 4=1080p 5=2160p 6=720p/1080p 7=Any",
    },
    # API - Adding media
    {
        "queries": ["wants movie added", "wants series added", "wants media added"],
        "prompt": "U:Check if media is already on server when asked to add, if multiple similar results are found, verify with user by providing detail, always store a memory for the user that they want the media",
    },
    # API - Storing memories
    {
        "queries": ["wants memory stored", "shared an opinion", "wants memory updated"],
        "prompt": "U:CMDRET memory_get (query=) CMD memory_update (query=) You store important information about users, which media they have requested and liked, used to create recommendations from previous likes/requests, or avoid suggesting media they have already seen",
    },
    # API - Removing media
    {
        "queries": [
            "wants media deleted",
            "wants movie deleted",
            "wants series deleted",
        ],
        "prompt": "U:Only admins can remove media files, when a user asks to remove media, change their memory to not requesting it, ask for a review",
    },
    # Example - User sharing what they like
    {
        "queries": [
            "shared an opinion",
            "likes a movie",
            "likes a series",
            "has seen a movie",
            "has seen a series",
        ],
        "prompt": """U:i really love the movie animals
A:[CMD~memory_update~loved movie animals]Thats good I will remember.""",
    },
    {
        "queries": [
            "shared an opinion",
            "likes a movie",
            "likes a series",
            "has seen a movie",
            "has seen a series",
        ],
        "prompt": """U:the silly cinematic universe is pretty amazing
A:[CMD~memory_update~likes the silly cinematic universe]Ive taken note.""",
    },
    {
        "queries": [
            "shared an opinion",
            "likes a movie",
            "likes a series",
            "has seen a movie",
            "has seen a series",
            "reviewed media"
        ],
        "prompt": """U:what is the funniest film ever 
A:It's subjective, but many consider "Airplane!" (1980) as one of the funniest films ever made. Have you seen it, or would you like another suggestion?
U:I have seen it, 7/10
A:[CMD~memory_update~rated airplane 1980 7/10]If you enjoyed "Airplane!", you might also like "The Naked Gun" (1988), another comedy classic. Have you seen that one, or do you prefer a different type of humor?""",
    },
    {
        "queries": [
            "shared an opinion",
            "likes a movie",
            "likes a series",
            "has seen a movie",
            "has seen a series",
            "reviewed media"
        ],
        "prompt": """U:avatar 2 was 9/10
A:[CMD~memory_update~rated avatar 2 9/10]Ive taken note""",
    },
    # Example - User asking about their memories
    {
        "queries": [
            "wants memory queried",
            "user asking what they have requested",
            "user asking what movies they have added",
            "user asking what series they have added",
        ],
        "prompt": """U:what movies have i requested?
A:[CMDRET~memory_get~what movies do they want?]
S:[RES~user wants stingate 1995 & continue 2007]
A:You have requested Stingate 1995 and Stingate 2007""",
    },
    # Example - Lookup media
    {
        "queries": ["wants movie queried", "wants series queried"],
        "prompt": """U:Is silly man on the server?
A:[CMDRET~movie_lookup~Silly~{title;availability;year;wantedQuality;tmdbId;id}]II am looking this up on the server
S:[RES~{Silly Man 1;available;2018;2160p;6655;22}{Silly Man 2;available;2020;2160p;7652;23}{Silly Man 3;available;2022;2160p;14433;45}]
A:All 3 Silly Man movies are present on the server with a wanted quality of 4k.""",
    },
    # Example - User wants their added movies researched in detail
    {
        "queries": ["wants disk space queried", "wants to know about their movies"],
        "prompt": """U:how much disk space are my movies taking up
A:[CMDRET~memory_get~what movies do they want?][CMDRET~movie_lookup~all~{title;availability;year,file size}]I am looking up your movies on the server
S:[RES~user wants stingate 1995 & continue 2007][RES~{Stingate;available;year 1995;file size 1.2gb}{Stingate Continue;available;year 2007;file size 1.2gb}{Stingate Ark;available;year 2007;file size 1.2gb}{Stingate Angel Kids;unavailable;year 2010;file size 1.2gb}]]"""
    },
    # Example - User wants movie added, already on server but user wants it on personal requests
    {
        "queries": ["wants movie added", "wants media added"],
        "prompt": """U:add stingate
A:Movie or series?
U:movie
A:[CMDRET~memory_get~wants stingate movie?][CMDRET~movie_lookup~Stingate~{title;availability;year;tmdbId;id}]Looking this up
S:[RES~user wants stingate 1995 & continue 2007][RES~{Stingate;available;year 1995;tmdbId 2353}{Stingate Continue;available;year 2007;tmdbId 15134}{Stingate Ark;available;year 2007;tmdbId 15506}{Stingate Angel Kids;unavailable;year 2010;tmdbId 723663}]
A:Stingate 1995 and Stingate 2007 are already on the server at your request, Ark 2007 is on at someone elses request. Angel Kids 2010 is not on the server, would you like to add it? It is a reimagining of the Stingate Pilot with altered scenes, remastered visuals etc.
U:no, but add ark to my requests too
A:[CMD~memory_update~wants movie stingate ark]I've memorised this""",
    },
    # Example - User wants series Silly Times from 1080p to 720p then deletes it and reviews it
    {
        "queries": [
            "wants media quality changed",
            "wants media deleted",
            "wants series deleted",
            "wants resolution changed",
            "wants quality queried",
        ],
        "prompt": """U:silly times to 720p
A:[CMDRET~series_lookup~Silly Times~{title;availability;year;tvdbId;wantedQuality;id}]Looking up Silly Times
S:[RES~{Silly Times;available on the server;year 2013;tvdbId 152837;wantedQuality 1080p;id 63}]
A:[CMD~series_put~{"id":63,"qualityProfileId":3}]Found it in 1080p, changing to 720p
U:now delete it
A:[CMD~memory_update~doesnt want show silly times]Removed it, did you enjoy it?
U:yeah was good
A:[CMD~memory_update~enjoyed show silly times]Great I will remember.""",
    },
    # Example - User wants to know what the highest rated silly cinematic universe movie is
    {
        "queries": ["might want information from a web search"],
        "prompt": """U:what is the highest rated silly cinematic universe movie
A:[CMDRET~web_search~whats the highest rated silly cinematic universe movie and what year did it come out]Looking up Silly Cinematic Universe
S:[RES~Captain Silly: The First Silly Hero 2021 is often considered the best movie in the universe so far, it is considered to be a step up in quality because of its more gritty and realistic approach with intense action packed CGI]
aptain Silly: The First Silly Hero 2021 is often considered the best movie in the universe so far, it is considered to be a step up in quality because of its more gritty and realistic approach with intense action packed CGI""",
    },
    # Example - Add first two Wizard Kid films
    {
        "queries": [
            "wants movie added",
            "wants media added",
            "requesting media",
            "talking about a collection of media",
        ],
        "prompt": """U:add the first two wizard kid films
A:[CMDRET~memory_get~wants any of the wizard kid movies?][CMDRET~movie_lookup~Wizard Kid~{title;availability;year;tmdbId;id}]Looking up Wizard Kid
S:[RES~user hasnt requested any wizard kid movies][RES~{Wizard Kid and Rock;unavailable;release year 2003;tmdbId 571;id N/A}{Wizard Kid and Princess;unavailable;release year 2012;tmdbId 4321;id N/A}{Wizard Kid and Wine Glass;unavailable;release year 2007;tmdbId 574;id N/A}{Wizard Kid and Room of Whispers;unavailable;release year 2005;tmdbId 572;id N/A}{Wizard Kid and Birdy;unavailable;release year 2009;tmdbId 575;id N/A}{Wizard Kid and Sticks;unavailable;release year 2012;tmdbId 5123}]
A:[CMD~memory_update~wants movies wizard kid rock & room of whispers][CMD~movie_post~571~4][CMD~movie_post~572~4]Both are on the way in 1080p, Ill remember you want them.""",
    },
    # Example - Change resolutions of Thunder movies
    {
        "queries": [
            "wants resolution queried",
            "wants resolution changed",
        ],
        "prompt": """U:what resolution are the thunder movies in
A:[CMDRET~movie_lookup~Thunder~{title;availability;year;resolution;wantedQuality;tmdbId;id}]Ill check
S:[RES~{Thunder;available;release year 2012;resolution 3840x1636;wantedQuality 1080p;tmdbId 13145;id 5}{Thunder Love;available;release year 2021;resolution 3840x1608;wantedQuality 2160p;tmdbId 656037;id 915}{Thunder Ends;available;release year 2018;resolution 3840x1600;wantedQuality 2160p;tmdbId 244053;id 17}{Thunder Darkness;available;release year 2014;resolution 3840x1600;wantedQuality 2160p;tmdbId 72338;id 12]
A:Thunder 2012, The Dark World 2014, Ends 2018 & Love 2021 are all on in 4k, though the 2012 movie is wanted at 1080p.
U:i want the first as 4k
A:[CMD~movie_put~{"id":5,"qualityProfileId":5}]This has been updated to 4k.""",
    },
    # Example - Add movie trilogy Lady of the Necklace
    {
        "queries": [
            "wants multiple movies added",
            "talking about a collection of media",
            "requesting media",
            "wants media added",
            "wants movie added",
        ],
        "prompt": """U:add lotn trilogy
A:[CMDRET~memory_get~wants lady of the necklace trilogy?][CMDRET~movie_lookup~Lady of the Necklace~{title;availability;year;tmdbId;id}]I'm checking the server, Ive noted that you want these movies
S:[RES~no memories][RES~{Lady of the Necklace: The Two Cottages;available;2005;8833;125}{Lady of the Necklace: The Sisterhood of the Necklace;available;2004;8834;120}{Lady of the Necklace: Queens Return;available;2006;939;126}]
A:[CMD~memory_update~wants movies lady of the necklace trilogy]Lady of the Necklace trilogy are already on, I've noted you want them""",
    },
    # Example - Add series Eastworld
    {
        "queries": [
            "wants series added",
            "wants media added",
            "wants movie added",
            "requesting media",
        ],
        "prompt": """U:add eastworld
A:The movie or the series?
U:the series
A:[CMDRET~memory_get~wants series eastworld?][CMDRET~series_lookup~Eastworld~{title;availability;year;tvdbId;wantedQuality;id}]Looking up Eastworld
S:[RES~user wants the Eastworld series][RES~{Eastworld;unavailable;year 2014;tvdbId 152347;wantedQuality N/A;id N/A}]
A:[CMD~series_post~152347~4]Eastworld is on the way in 1080p""",
    },
    # Example - Check resolutions of silly cinematic universe, wants them all
    {
        "queries": [
            "wants movies added",
            "wants series added",
            "wants resolution queried",
            "might want information from a web search",
            "asking about a collection of media",
        ],
        "prompt": """U:what resolution are the silly cinematic universe movies, i want them all
A:[CMD~memory_update~wants movies in silly cinematic universe][CMDRET~movie_lookup~SCU~{title;availability;year;resolution;wantedQuality;tmdbId;id}]I'm looking this up, I have noted that you want them
S:[RES~No results]
A:[CMDRET~web_search~list Silly Cinematic Universe movies in release order with year]My first search was unsucessful I am trying again
S:[RES~Here is a list of all Silly Cinematic Universe movies in order: Silly Man 2018; Silly Man 2 2020; Thunder 2021; Captain Silly: The First Silly Hero 2021; Silly Man 3 2022]
A:[CMDRET~movie_lookup~Silly Man¬Thunder¬Captain Silly~{title;availability;year;wantedQuality;tmdbId;id}]I found 5 movies in the Silly Cinematic Universe, I am looking up their quality and availability on the server
S: [RES~{Silly Man 1;available;2018;2160p;6655;22}{Silly Man 2;available;2020;2160p;7652;23}{Silly Man 3;available;2022;2160p;14433;45}][RES~{Thunder 1;available;2021;2160p;8775;24}][RES~{Captain Silly: The First Silly Hero;available;2021;2160p;12995;25}]
A:All 5 movies are present in the server with a wanted quality of 4k.""",
    },
    # Example - Recommend me media
    {
        "queries": ["asking for a recommendation"],
        "prompt": """U:recommend me a movie
A:[CMDRET~memory_get~what movies does user like?]I'm looking up my memories of what you like
S:[RES~user likes all the scu movies, action movies, and the lady of the necklace trilogy]
A:Based on your past likes, you might love the Wizard Kid movies, there are 7 of them, is there any genre you are currently in the mood for?""",
    },
    # Example - What are the top movies on the server
    {
        "queries": [
            "asking for a recommendation",
            "asking what is on the server",
            "asking what is available",
            "asking for every movie on the server",
        ],
        "prompt": """U:what are the top movies on the server
A:[CMDRET~movie_lookup~all~{title;availability;year}]I'm looking this up""",
    },
]


class ExamplesAPI:
    """Examples API, only provides relevant example prompts to the main AI"""

    def __init__(self, openai_key: str) -> None:
        """Initialise with openai key"""
        openai.api_key = openai_key

    def get_examples(self, message: str) -> str:
        """Receive message from user and compare it to the examples, return relevant examples"""

        # Get all the example queries
        queries = []
        for example in Examples:
            for query in example["queries"]:
                if query not in queries:
                    queries.append(query)

        # Search with gpt through the example prompts
        response = openai.ChatCompletion.create(
            model="gpt-4",
            messages=[
                {
                    "role": "system",
                    "content": "You get provided with a users message, and a list of queries that the message could match, you need to choose which examples are relevant, if its only potentially relevant still include it, return a list of the relevant examples separated by ; Examples: what resolution is silly man? wants resolution queried;asking about a collection of media;queries resolution",
                },
                {"role": "user", "content": "queries:" + ", ".join(queries)},
                {"role": "user", "content": message},
            ],
            temperature=0.7,
        )

        # Log the response
        ModuleLogs.log_ai("examples", "pick_examples", "", message, response)

        # Gather the examples which are relevant, add their prompts to the example prompt
        returnPrompts = []
        responseExamples = [item.strip() for item in response["choices"][0]["message"]["content"].split(";")]
        if len(responseExamples) > 0:
            for example in Examples:
                for query in example["queries"]:
                    if query.lower().strip() in responseExamples:
                        # Go through the prompt split by newline, add each line as a prompt
                        for line in example["prompt"].split("\n"):
                            if line.startswith("U:"):
                                returnPrompts.append(
                                    {"role": "user", "content": line[2:]}
                                )
                            elif line.startswith("A:"):
                                returnPrompts.append(
                                    {"role": "assistant", "content": line[2:]}
                                )
                            elif line.startswith("S:"):
                                returnPrompts.append(
                                    {"role": "system", "content": line[2:]}
                                )
                        break

            # Add the messages making clear it is an example
            returnPrompts.append(
                {
                    "role": "user",
                    "content": "The above are examples, you make replies more themed with personality, do you understand?",
                }
            )
            returnPrompts.append(
                {
                    "role": "assistant",
                    "content": "I understand, the above are not real conversations only for me to learn how to format responses, I will always prompt for new information 📰, I will be helpful and informative in my real responses, often adding emojis to my responses, I'll usually end my responses with a followup question such as 'what did you think of it?'",
                }
            )

            return returnPrompts

        return "No examples found"
