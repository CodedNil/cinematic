import json
import openai
from modules.module_logs import ModuleLogs

from duckduckgo_search import ddg
from trafilatura import fetch_url, extract


class WebAPI:
    """Class to handle web search and scraping"""

    def __init__(self, openai_key: str) -> None:
        """Initialise with credentials"""
        openai.api_key = openai_key

        # Create logs
        self.logs = ModuleLogs("web")

    async def basic(self, query: str = "", numResults: int = 4) -> dict:
        """Perform a DuckDuckGo Search and return the results as a JSON string"""
        search_results = []
        if not query:
            return json.dumps(search_results)

        results = ddg(query, max_results=numResults)
        if not results:
            return json.dumps(search_results)

        for j in results:
            search_results.append(j)

        return search_results

    async def advanced(self, query: str = "") -> str:
        """Perform a DuckDuckGo Search, then scrape the sites through gpt to return the answer to the prompt"""
        search_results = await self.basic(query, 8)

        # Go through each site until we get a good answer
        for website in search_results:
            # Scrape the site, fetch only the main content
            url = website["href"]
            downloaded = fetch_url(url)
            if downloaded:
                # Extract text data from website
                result = extract(downloaded)

                if len(result) > 4096:
                    result = result[:4096]

                # Run a chat completion to get the answer to the prompt from the summarised text
                response = openai.ChatCompletion.create(
                    model="gpt-3.5-turbo",
                    messages=[
                        {
                            "role": "user",
                            "content": f"{result}\nAbove is the summary of the website {url}, give an answer to '{query}, if the context is insufficient, reply 'no answer'",
                        },
                    ],
                    temperature=0.7,
                )
                # Log the response
                self.logs.log("answer", result.replace("\n", " "), query, response)

                # Check if the response is valid
                responseMessage = response["choices"][0]["message"]["content"].strip()
                if responseMessage.lower() not in ["no answer", "no answer.", "no answer!"]:
                    return responseMessage

        return "Could not find an answer to your question"
