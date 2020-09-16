<?PHP
/*
  Wikimedia Commons image feed
  (c) 2013 by Magnus Manske
  Released under GPL
*/

// RSS format example : http://api.flickr.com/services/feeds/photos_public.gne?tags=nature&format=rss2
// other : http://www.degraeve.com/flickr-rss/rss.php?tags=brasilia+architecture&tagmode=all&sort=interestingness-desc&num=25

error_reporting(E_ERROR|E_CORE_ERROR|E_ALL|E_COMPILE_ERROR);
ini_set('display_errors', 'On');

require_once ( "php/ToolforgeCommon.php" ) ;
$tfc = new ToolforgeCommon('catfood') ;

function get_image_url ( $lang , $image , $project = "wikipedia" ) {
  global $tfc ;
  $wiki = $tfc->getWikiForLanguageProject ( $lang , $project ) ;
  return "//".getWebserverForWiki($wiki)."/wiki/Special:Redirect/file/".$tfc->urlEncode($image);
}

function get_thumbnail_url ( $lang , $image , $width , $project = "wikipedia" ) {
  global $tfc ;
  $wiki = $tfc->getWikiForLanguageProject ( $lang , $project ) ;
  return "//".getWebserverForWiki($wiki)."/wiki/Special:Redirect/file/".$tfc->urlEncode($image)."?width={$width}";
}

function xml_safe ( $s ) {
	$doc = new DOMDocument();
	$fragment = $doc->createDocumentFragment();
	$fragment->appendChild($doc->createTextNode($s));
	$ret = $doc->saveXML($fragment);
	return $ret ;
}

$depth = 0 ; // Untested
$thumb_width = 75 ;
$max_images = $tfc->getRequest ( 'max_images' , 100 ) ;
$category = $tfc->getRequest ( 'category' , '' ) ; //'Varanus komodoensis' ;

if ( $category == '' ) {
	print $tfc->getCommonHeader ( 'Commons image feed' ) ;
	print "<div>Generates RSS feeds for screensavers from Wikimedia Commons categories (<a href='./commons_image_feed.php?category=Varanus%20komodoensis'>example</a>).</div>" ;
	print "<form method='get' class='form-inline' action='?'><table class='table table-condensed'><tbody>" ;
	print "<tr><th>Category</th><td><input name='category' type='text' value='$category' /></td><td/></tr>" ;
	print "<tr><th nowrap>Max images</th><td><input name='max_images' type='text' value='$max_images' /></td><td><small>(image order is randomized, so you always get new images from a large category even with a relatively low number)</small></td></tr>" ;
	print "<tr><td/><td><input type='submit' class='btn btn-primary' value='Gimme RSS!' /></td><td/></tr>" ;
	print "</tbody></table></form>" ;
	print $tfc->getCommonFooter() ;
	exit ( 0 ) ;
}


date_default_timezone_set('UTC');
$now = date('r') ;

//header('Content-type: text/plain; charset=utf-8');
header('Content-type: text/xml; charset=utf-8');
header("Cache-Control: no-cache, must-revalidate"); // HTTP/1.1
header("Expires: Sat, 26 Jul 1997 05:00:00 GMT"); // Date in the past


print "<rss xmlns:media='http://search.yahoo.com/mrss/' xmlns:dc='http://purl.org/dc/elements/1.1/' xmlns:creativeCommons='http://cyber.law.harvard.edu/rss/creativeCommonsRssModule.html' version='1.0'>
<channel>
<title>Category \"" . xml_safe($category) . "\" on Wikimedia Commons</title>
<link>http://commons.wikimedia.org/wiki/Category:" . $tfc->urlEncode($category) . "</link>
<description/>
<pubDate>$now</pubDate>
<lastBuildDate>$now</lastBuildDate>
<generator>http://toolserver.org/~magnus/commons_image_feed.php</generator>
" ;

$db = $tfc->openDB ( 'commons' , "wikimedia" ) ;

$cat = $db->real_escape_string ( $category ) ;
$cat = str_replace ( " " , "_" , $cat ) ;
if ( $depth == 0 ) $cat = '"'.$cat.'"' ;
else {
	$cats = [] ;
  	$tfc->findSubcats ( $db , [ $cat2 ]  , $cats , $depth ) ; // FIXME!!!	
  	//$cat = db_get_articles_in_category ( 'commons' , $cat , $depth-1 , 14 ) ;
	$cat = '"' . implode ( '","' , $cats ) . '"' ;
}

$max_images *= 1 ;
$sql = "select image.* from image,page,categorylinks where page_title=img_name AND cl_from=page_id AND cl_to IN ($cat) AND cl_type='file' AND img_media_type='BITMAP' ORDER BY rand() LIMIT $max_images" ;
$result = $tfc->getSQL ( $db , $sql ) ;
while($o = $result->fetch_object()){
	$name = $o->img_name ;
	$desc = $o->img_description ;
	$meta = $o->img_metadata ;
	$user = $o->img_user_text ;
	$type = $o->img_minor_mime ;
	$img_width = $o->img_width ;
	$img_height = $o->img_height ;
	
	$nice_title = str_replace ( '_' , ' ' , $name ) ;
	$nice_title = preg_replace ( '/\.[a-z]+$/i' , '' , $nice_title ) ;
	$nice_title = xml_safe ( $nice_title ) ;
	
	$page_url = "http://commons.wikimedia.org/wiki/File:" . $tfc->urlEncode ( $name ) ;
	$image_url = get_image_url ( 'commons' , $name , "wikimedia" ) ;
	$thumb_url = get_thumbnail_url ( 'commons' , $name , $thumb_width , "wikimedia" ) ;
	
	print "<item>" ;
	print "<title>$nice_title</title>" ;
	print "<link>$page_url</link>" ;
	print "<description><img src='$image_url' alt='' /></description>" ;
	print "<pubDate>$now</pubDate>" ;
//	print "<dc:date.Taken>2013-04-10T08:58:39-08:00</dc:date.Taken>
	// author
	print "<guid>$image_url</guid>" ;
	print "<media:title>$nice_title</media:title>" ;
	print "<media:content url='$image_url' type='image/$type' height='$img_height' width='$img_width'/>" ;
	// <media:title>Rinnsal | trickle</media:title>
	// <media:description type="html">
	print "<media:thumbnail url='$thumb_url' width='$thumb_width' />" ;
	print "<media:credit role='uploader'>" . xml_safe ( $user ) . "</media:credit>" ;
	print "<enclosure url='$image_url' type='image/$type'/>" ;
	print "</item>" ;
}


print "</channel></rss>" ;
myflush() ;

$tfc->logToolUse('','commons image feed rss') ;

?>